# CuantoQuedapp

[English](README.md) · **Español**

*El nombre viene de «¿cuánto queda?».*

Un ícono en la barra de menú del Mac (y en la bandeja de Windows) que muestra cuánto llevas usado de los límites de **Codex** y de **Claude Code**: la ventana de 5 horas y la semanal, con la hora a la que se reinicia cada una.

La app habla español e inglés: sigue el idioma del sistema, y puedes elegir uno en Ajustes.

```
[>_] 54%  [✱] 45%              ← en la barra de menú: Codex y Claude (ventana de 5 horas)

Clic izquierdo → panel con barras de color:

  [>_] Codex  plus                 dato de hace 0 min
  5 horas                                             54%
  ████████████░░░░░░░░░░
  se reinicia en 3 h 40 min (sáb 21-nov 00:23)
  Semana                                              27%
  █████░░░░░░░░░░░░░░░░░
  se reinicia en 6 d 11 h (jue 26-nov 20:46)

  [✱] Claude                       dato de hace 0 min
  ...
  ─────────────────────────────
  Actualizar ahora
  Ajustes                                              ›
  Salir
```

CuantoQuedapp en sí no envía nada a ningún servidor: no tiene código de red propio, no guarda ni lee contraseñas ni tokens, y solo interpreta los campos de límites (porcentajes y horas), nunca el contenido de tus conversaciones. Para el dato en vivo ejecuta tus propios Codex y Claude Code en este equipo, y ellos se conectan a sus servicios como siempre, con su propio inicio de sesión y ajustes.

CuantoQuedapp es un proyecto independiente, sin relación con OpenAI ni Anthropic ni respaldado por ellas. Codex y Claude son marcas de sus dueños.

## Qué necesitas

| Para ver… | Necesitas |
|---|---|
| **Codex** | Tener Codex instalado y haberlo usado alguna vez con tu cuenta de ChatGPT |
| **Claude** | Tener Claude Code instalado, con la sesión iniciada con tu **suscripción** (Pro, Max…): `claude auth login` |
| **El Mac** | macOS 11 o superior con **Apple Silicon** (M1 o más nuevo). En un Mac Intel hay que compilarla (ver más abajo) |

Con una sola de las dos herramientas funciona igual: solo aparece la que tengas.

## Instalar en el Mac

1. Descarga `CuantoQuedapp-<versión>-macos-arm64.zip` desde [Releases](https://github.com/javieer133/cuantoquedapp/releases), o compílala tú (ver «Compilar desde el código»).
2. Haz doble clic en el zip para descomprimirlo y arrastra **CuantoQuedapp** a la carpeta **Aplicaciones**.
3. Ábrela. La primera vez macOS la bloqueará porque no está firmada con un certificado de Apple de pago (se firma de forma local). Para abrirla una vez:
   - **Opción A:** en **Ajustes del Sistema → Privacidad y seguridad**, baja hasta el aviso sobre «CuantoQuedapp» y pulsa **Abrir de todos modos**.
   - **Opción B (Terminal):** `xattr -dr com.apple.quarantine "/Applications/CuantoQuedapp.app"` y vuelve a abrirla.
4. Aparece en la barra de menú (no tiene ícono en el Dock). Espera un par de segundos mientras lee los datos.

Si quieres comprobar que el archivo no se alteró, compara su huella con el `.sha256` publicado junto a él: `shasum -a 256 CuantoQuedapp-*-macos-arm64.zip`.

## Cómo se usa

- **Clic izquierdo** en el ícono: abre el panel con las barras de cada herramienta y el tiempo que falta para que se reinicie cada ventana. Se cierra al hacer clic fuera o con Esc. Al pie: «Actualizar ahora», «Ajustes» y «Salir».
- **Ajustes** (en el panel; el menú del clic derecho también tiene «Mostrar» y «Alerta al superar»): qué ventanas y herramientas mostrar, qué número va en la barra, alerta al superar el 50, 75 o 90 %, tema claro u oscuro (o el del sistema), el idioma (el del sistema, Español o English) e «Iniciar al abrir sesión».
- **Iniciar al abrir sesión**: hace que la app arranque sola al entrar al Mac. Solo está disponible con la app instalada en Aplicaciones. macOS puede mostrar un aviso de «elemento en segundo plano»: es normal.
- Los colores avisan del consumo: verde por debajo del 60 %, ámbar hasta el 85 %, rojo por encima.
- La alerta: en la barra de menú, el número de una herramienta se pone rojo cuando una de sus ventanas visibles (5 horas o semana) llega al nivel elegido; en Windows el tooltip le antepone `⚠`.
- La app se actualiza sola: Codex cada minuto, Claude cada 5 minutos como máximo, y las dos justo después de que se reinicia una ventana.
- Abrir la app por segunda vez no crea un segundo ícono.

### Qué significa el texto de la barra

| Texto | Significado |
|---|---|
| `[>_] 54%` | Has usado el 54 % de la ventana de 5 horas de Codex |
| `[✱] 45%` | Lo mismo para Claude |
| `… · dato antiguo` | El último dato tiene más de una hora |
| `… · aviso` | Hay algo que revisar en el panel (por ejemplo, un formato que no se entendió) |
| `[✱] ↻` | Esa ventana acaba de reiniciarse: el número viejo se oculta y la app ya está pidiendo el nuevo (tarda unos segundos) |
| `?% · desactualizado` | El dato tiene fecha futura: revisa el reloj del equipo |
| `Codex ?% · sin lectura` | El lector interno se detuvo; reinicia la app |
| `Codex ?%` | Todavía no hay datos |

## Si algo no sale

| Síntoma | Qué hacer |
|---|---|
| macOS dice que la app está dañada o no se puede abrir | Es el bloqueo de la primera vez: ver los pasos 3A o 3B de «Instalar» |
| Solo aparece Codex y no Claude | Comprueba en Terminal que `claude -p "/usage" --no-session-persistence` imprime «Current session…». Si imprime costes («Total cost…»), Claude Code usa una **clave de API** y no tu suscripción: ejecuta `claude auth login` con tu cuenta de suscripción. La app ya ignora la variable `ANTHROPIC_API_KEY` al lanzar `claude`, pero necesita que la sesión de suscripción exista |
| Solo aparece Claude y no Codex | Abre Codex una vez y usa algo, para que cree sus datos. Si lo tienes en una ruta distinta, define `CUANTOQUEDAPP_CODEX` con la ruta absoluta del ejecutable |
| El número parece atrasado | El dato de Codex lo pide la app cada minuto y el de Claude cada 5; si el título dice «dato antiguo», Codex o Claude no respondieron |
| Quiero ver qué está pasando | Ciérrala y ábrela desde Terminal con `CUANTOQUEDAPP_DEBUG=1 "/Applications/CuantoQuedapp.app/Contents/MacOS/cuantoquedapp"`: escribe `debug.log` (solo eventos y geometría, sin datos) en `~/Library/Caches/cuantoquedapp/` |

## Desinstalar

1. En el menú, desmarca «Iniciar al abrir sesión» y elige «Salir».
2. Borra `/Applications/CuantoQuedapp.app`.
3. Opcional: borra la carpeta `~/Library/Caches/cuantoquedapp` (guarda un archivo de bloqueo, el último dato de Claude del respaldo y, si lo activaste, el registro de depuración).
4. Si instalaste el respaldo de Claude: `scripts/claude-statusline.sh uninstall`.

## Windows

Descarga `CuantoQuedapp-<versión>-windows-x64.zip` desde [Releases](https://github.com/javieer133/cuantoquedapp/releases) y descomprímelo en una carpeta fija, o compílala tú. El soporte de Windows todavía es experimental: no ha pasado toda la lista de pruebas de [docs/windows.md](docs/windows.md). El `.exe` no necesita nada aparte (tampoco el Visual C++ Redistributable); lo que Windows necesita para ejecutarlo, y el aviso de SmartScreen de la primera vez, están en [docs/windows.md](docs/windows.md#running-it). La guía paso a paso (requisitos, compilación, pruebas, desinstalación y qué hacer si algo falla) está en [docs/windows.md](docs/windows.md), en inglés. En Windows la bandeja no muestra texto: el porcentaje de Codex se dibuja dentro del ícono (o el de Claude si no hay Codex), y el detalle sale al pasar el ratón y en el panel del clic izquierdo.

## Compilar desde el código

Útil si tienes un Mac Intel o quieres modificarla. Necesitas [Rust](https://rustup.rs), Git y [cargo-about](https://github.com/EmbarkStudios/cargo-about) (`cargo install cargo-about --locked --features cli`), que escribe dentro de la app los avisos de licencias de terceros:

```
git clone --branch main https://github.com/javieer133/cuantoquedapp.git
cd cuantoquedapp
scripts/package-macos.sh              # crea dist/CuantoQuedapp.app
scripts/package-macos.sh --install    # además la copia a /Applications
scripts/package-macos.sh --zip        # además crea el zip y su huella para compartir
```

`main` tiene la última versión publicada; `develop`, la rama por defecto, el trabajo en curso. Se puede combinar (`--install --zip`). La app queda firmada de forma local («ad hoc»), por lo que funciona sin avisos en el Mac que la compiló. Para repartirla a otros Macs sin el aviso de la primera vez haría falta una firma Developer ID de Apple y notarización (de pago, no incluidas).

## En la terminal

```
cargo run --bin cuantoquedapp-cli                       # los mismos datos, solo de archivos de Codex
cargo run --bin cuantoquedapp-cli -- --live             # Codex y Claude en vivo, como el ícono
cargo run --bin cuantoquedapp-cli -- --folder RUTA       # otra carpeta de sesiones de Codex
cargo run --bin cuantoquedapp-cli -- --now 2026-11-20T12:00:00Z   # hora fija (pruebas)
```

## Cómo obtiene los datos

- **Codex**: lee los límites que Codex ya guarda en `~/.codex/sessions` (o en `$CODEX_HOME/sessions`) y, además, se los pide en vivo al propio Codex (`codex app-server`, un proceso local que la app abre y cierra). Si el ejecutable nativo no se encuentra, usa solo los archivos.
- **Claude**: ejecuta `claude -p "/usage" --no-session-persistence`, una orden local de Claude Code (no llama al modelo, no cuesta nada y no deja sesiones). Usa el inicio de sesión del propio Claude Code: la app nunca ve ningún token. Al lanzar `claude` le quita, solo a ese proceso, `ANTHROPIC_API_KEY` y `ANTHROPIC_AUTH_TOKEN`, porque con ellas Claude Code muestra costes de API en vez de los límites de tu suscripción.
- **Respaldo opcional para Claude (Mac y Linux)**: `scripts/claude-statusline.sh install` encadena un pequeño ayudante delante del `statusLine` de Claude Code para guardar sus límites cuando hay una sesión de terminal abierta. Hace una copia de respaldo de `~/.claude/settings.json` y se deshace con `uninstall` (`status` muestra si está instalado). Solo se usa si `/usage` no responde, y puede ir atrasado.
- Variables de entorno: `CUANTOQUEDAPP_CODEX` y `CUANTOQUEDAPP_CLAUDE` (ruta absoluta del ejecutable; las rutas relativas, y las carpetas relativas del `PATH`, se ignoran), `CUANTOQUEDAPP_DEBUG` (activa el registro), `CUANTOQUEDAPP_LANG` (`es` o `en`: fuerza el idioma, por encima del ajuste y del sistema).

## Límites conocidos

- Depende de formatos que **no están documentados** (los archivos de Codex, `codex app-server` y el texto de `/usage` de Claude Code) y pueden cambiar con una actualización. Si pasa, la app lo dice («No pude leer el formato…») o deja de mostrar esa herramienta; nunca inventa un número.
- Los datos de archivos son los de este equipo; el dato en vivo refleja también lo que gastaste desde otro equipo o la web.
- Las horas de reinicio de Claude se leen en la zona horaria del sistema.
- Los íconos de la barra son dibujos propios, no los logotipos oficiales de OpenAI ni de Anthropic.
- Windows compila pero se ha probado poco; ver [docs/windows.md](docs/windows.md).

## Desarrollo

```
cargo test                                            # pruebas (fixtures a mano, nunca ~/.codex)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo check --target x86_64-pc-windows-msvc           # comprobar Windows desde el Mac (rustup target add ...)
cargo test real_launch_agent -- --ignored            # prueba real del autoarranque (toca ~/Library/LaunchAgents)
```

Los textos de la interfaz están en `locales/app.yml`, con español e inglés lado a lado; una prueba falla si a un texto le falta un idioma. Ejemplos de apoyo en `examples/`: `merged_report` (lo que mostraría el ícono), `panel_html` (la página del panel), `show_report`, `measure`, `view_icon`, `generate_icon`.

`src/lib.rs` agrupa la lógica sin interfaz, `src/tray/` y `src/panel*.rs` son la interfaz, y `src/bin/` tiene los programas. Cómo contribuir (ramas, comprobaciones, versiones): [CONTRIBUTING.md](CONTRIBUTING.md), en inglés.

## Licencia

A tu elección, bajo cualquiera de estas dos:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- Licencia MIT ([LICENSE-MIT](LICENSE-MIT))

Las licencias de los crates de código abierto con que se construye CuantoQuedapp están en `THIRD-PARTY-LICENSES.html`, dentro de la app (`CuantoQuedapp.app/Contents/Resources/`) y junto al `.exe` de Windows; `about.toml` lista las licencias aceptadas.

Salvo que digas lo contrario, toda contribución que envíes para incluirla en el proyecto, según la define la licencia Apache-2.0, queda bajo esas mismas dos licencias, sin condiciones adicionales.
