//! The background reader of the tray icon and its supervisor.
//!
//! `run` is the loop of the reading thread: read, send the report, then wait for the next
//! round or for a `RefreshNow` order. `Supervisor` is the interface-side bookkeeping: it notices
//! a reader that stopped delivering, and it limits manual refreshes. Both take their clock
//! and their reading function as parameters, so they are tested without threads or processes.
use std::{
    io,
    path::PathBuf,
    sync::Arc,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, channel, sync_channel},
    time::{Duration, Instant},
};

use chrono::{Local, Utc};

use crate::{
    monitor::{self, Report},
    session_cache::SessionCache,
    sources::{SharedTools, Sources},
};

/// Time between automatic readings.
pub const INTERVAL: Duration = Duration::from_secs(60);
/// Without any report for this long, the reader is considered stuck and replaced.
pub const STALL_LIMIT: Duration = Duration::from_secs(300);
/// Minimum time between two manual refreshes.
pub const MANUAL_COOLDOWN: Duration = Duration::from_secs(5);
/// Minimum time between two replacements of the reader.
pub const RESTART_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Read everything now, also what is normally rate limited.
    RefreshNow,
}

/// Loop of the reading thread. `read(force)` makes one report; it returns when the interface
/// stops listening (either channel closed).
pub fn run(
    mut read: impl FnMut(bool) -> Report,
    orders: Receiver<Order>,
    reports: SyncSender<Report>,
    interval: Duration,
) {
    let mut force = false;
    loop {
        if reports.send(read(force)).is_err() {
            return;
        }
        force = match orders.recv_timeout(interval) {
            Ok(Order::RefreshNow) => {
                // Several orders queued up count as one.
                while orders.try_recv().is_ok() {}
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };
    }
}

/// Handle to a running reading thread.
pub struct Reader {
    pub reports: Receiver<Report>,
    orders: Sender<Order>,
}

impl Reader {
    /// Starts the thread. With `sources` it also asks Codex and Claude live; without them
    /// (custom `--folder`) it only reads the session files. `tools` says which ones to ask
    /// live, and the interface may change it at any time.
    pub fn spawn(
        folder: PathBuf,
        sources: Option<Sources>,
        tools: Arc<SharedTools>,
    ) -> io::Result<Reader> {
        let (report_sender, reports) = sync_channel(1);
        let (orders, order_receiver) = channel();
        let mut sources = sources;
        // Session files are read incrementally: only what was appended since the last round.
        let mut cache = SessionCache::new();
        std::thread::Builder::new()
            .name("limits-reader".into())
            .spawn(move || {
                run(
                    |force| {
                        let live = sources
                            .as_mut()
                            .map(|sources| sources.poll(Utc::now(), &Local, force, tools.get()))
                            .unwrap_or_default();
                        monitor::query_cached(&folder, live, &mut cache, Local::now().date_naive())
                    },
                    order_receiver,
                    report_sender,
                    INTERVAL,
                );
            })?;
        Ok(Reader { reports, orders })
    }

    /// `false` if the thread is already gone.
    pub fn refresh_now(&self) -> bool {
        self.orders.send(Order::RefreshNow).is_ok()
    }
}

/// Interface-side state: when the last report came, whether a manual refresh is pending.
#[derive(Debug)]
pub struct Supervisor {
    last_report: Instant,
    last_restart: Option<Instant>,
    last_manual: Option<Instant>,
    refreshing: bool,
}

impl Supervisor {
    pub fn new(now: Instant) -> Self {
        Supervisor {
            last_report: now,
            last_restart: None,
            last_manual: None,
            refreshing: false,
        }
    }

    pub fn on_report(&mut self, now: Instant) {
        self.last_report = now;
        self.refreshing = false;
    }

    /// True while a manual refresh waits for its report.
    pub fn is_refreshing(&self) -> bool {
        self.refreshing
    }

    /// Whether a manual refresh order should be sent now (and, if so, marks it as pending).
    /// Ignored while one is pending and within `MANUAL_COOLDOWN` of the previous one.
    pub fn request_manual_refresh(&mut self, now: Instant) -> bool {
        if self.refreshing
            || self
                .last_manual
                .is_some_and(|at| now.duration_since(at) < MANUAL_COOLDOWN)
        {
            return false;
        }
        self.last_manual = Some(now);
        self.refreshing = true;
        true
    }

    /// Whether the reader should be replaced: it delivered nothing for `STALL_LIMIT`
    /// (or it is gone) and it was not replaced just now.
    pub fn should_restart(&self, now: Instant, reader_gone: bool) -> bool {
        let stalled = now.duration_since(self.last_report) >= STALL_LIMIT;
        (stalled || reader_gone)
            && self
                .last_restart
                .is_none_or(|at| now.duration_since(at) >= RESTART_COOLDOWN)
    }

    /// Call after replacing the reader: restarts the waiting period.
    pub fn restarted(&mut self, now: Instant) {
        self.last_restart = Some(now);
        self.last_report = now;
        self.refreshing = false;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use super::*;

    fn empty_report() -> Report {
        monitor::reader_lost_report()
    }

    // C3: an order wakes the loop at once and forces the next reading.
    #[test]
    fn a_refresh_order_reads_again_immediately_with_force() {
        let calls = Arc::new(AtomicUsize::new(0));
        let forced = Arc::new(AtomicUsize::new(0));
        let (order_sender, orders) = channel();
        let (report_sender, reports) = sync_channel(1);
        let (calls2, forced2) = (Arc::clone(&calls), Arc::clone(&forced));
        let handle = thread::spawn(move || {
            run(
                move |force| {
                    calls2.fetch_add(1, Ordering::SeqCst);
                    if force {
                        forced2.fetch_add(1, Ordering::SeqCst);
                    }
                    empty_report()
                },
                orders,
                report_sender,
                // Long enough that only the order can explain a second reading.
                Duration::from_secs(60),
            );
        });
        reports.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            forced.load(Ordering::SeqCst),
            0,
            "first reading is not forced"
        );
        order_sender.send(Order::RefreshNow).unwrap();
        order_sender.send(Order::RefreshNow).unwrap();
        reports.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(forced.load(Ordering::SeqCst), 1);
        drop(reports);
        drop(order_sender);
        handle.join().unwrap();
        assert!(calls.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn the_loop_reads_again_after_the_interval_and_stops_when_nobody_listens() {
        let (_order_sender, orders) = channel();
        let (report_sender, reports) = sync_channel(1);
        let handle = thread::spawn(move || {
            run(
                |_| empty_report(),
                orders,
                report_sender,
                Duration::from_millis(20),
            );
        });
        for _ in 0..3 {
            reports.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        drop(reports);
        handle.join().unwrap();
    }

    #[test]
    fn the_loop_stops_when_the_order_channel_closes() {
        let (order_sender, orders) = channel();
        let (report_sender, reports) = sync_channel(1);
        let handle = thread::spawn(move || {
            run(
                |_| empty_report(),
                orders,
                report_sender,
                Duration::from_secs(60),
            );
        });
        reports.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(order_sender);
        handle.join().unwrap();
    }

    // C2
    #[test]
    fn a_silent_reader_is_replaced_after_the_limit_but_a_healthy_one_is_not() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(start);
        assert!(!supervisor.should_restart(start + Duration::from_secs(299), false));
        assert!(supervisor.should_restart(start + STALL_LIMIT, false));
        // A report resets the wait.
        supervisor.on_report(start + Duration::from_secs(200));
        assert!(!supervisor.should_restart(start + Duration::from_secs(400), false));
        assert!(supervisor.should_restart(start + Duration::from_secs(500), false));
    }

    #[test]
    fn a_gone_reader_is_replaced_but_not_in_a_loop() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(start);
        assert!(supervisor.should_restart(start + Duration::from_secs(1), true));
        supervisor.restarted(start + Duration::from_secs(1));
        // It died again at once: wait before trying again.
        assert!(!supervisor.should_restart(start + Duration::from_secs(2), true));
        assert!(supervisor.should_restart(start + Duration::from_secs(2) + RESTART_COOLDOWN, true));
    }

    // C3
    #[test]
    fn manual_refresh_is_ignored_while_pending_and_rate_limited() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(start);
        assert!(supervisor.request_manual_refresh(start));
        assert!(supervisor.is_refreshing());
        assert!(
            !supervisor.request_manual_refresh(start + Duration::from_secs(1)),
            "pending"
        );
        supervisor.on_report(start + Duration::from_secs(2));
        assert!(!supervisor.is_refreshing(), "cleared by the report");
        assert!(
            !supervisor.request_manual_refresh(start + Duration::from_secs(3)),
            "cooldown"
        );
        assert!(supervisor.request_manual_refresh(start + MANUAL_COOLDOWN));
    }

    #[test]
    fn spawn_without_sources_reads_the_session_files_and_answers_refresh() {
        let folder = std::env::temp_dir().join("cuantoquedapp-reader-does-not-exist");
        let reader = Reader::spawn(
            folder,
            None,
            Arc::new(SharedTools::new(crate::sources::Tools::ALL)),
        )
        .unwrap();
        reader
            .reports
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(reader.refresh_now());
        reader
            .reports
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
    }
}
