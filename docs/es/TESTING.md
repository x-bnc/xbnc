# Pruebas

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Entorno de pruebas](#test-environment)
- [Informe para pruebas](#test-report)
- [Probar la beta](#beta)

<a id="test-environment"></a>

## Entorno de pruebas

### Con Docker

`testenv/` levanta xBNC con dos servidores IRC distintos (Ergo e InspIRCd) en una red aislada, todo en texto plano a propósito:

```sh
cd testenv
docker compose up -d --build
./setup.sh
# IRC: localhost:6667, contraseña test/ergo@laptop:testpassword1
# Panel: http://localhost:8080 (usuario test, contraseña testpassword1)
```

No publiques esos puertos en una interfaz pública.

### Tests automáticos

```sh
cargo test --workspace                                   # todos los tests
XBNC_TEST_POSTGRES=postgres://postgres@127.0.0.1/postgres cargo test --workspace   # también con PostgreSQL
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm run build && npm run i18n:check            # panel y traducciones
xbncd locales check locales/*.json                       # ficheros de idioma
python3 ../xbnc-wiki/build.py --app .                   # la wiki (proyecto aparte)
```

Incluyen el protocolo, las unidades, la API contra un bouncer real, IRC de extremo a extremo, todos los comandos de `*status`, el cambio a un servidor de reserva, el certificado de confianza con un saludo TLS real, un idioma añadido como fichero, módulos y plugins contra un servidor IRC simulado, el panel, SSO y servidores IRC reales en el CI (Ergo, InspIRCd, UnrealIRCd, Solanum y miniircd). El *fuzzing* (`fuzz/`) se ejecuta cada semana. `testenv/ui/language_check.py` comprueba en un navegador real que todas las formas de cambiar el idioma funcionan.

### Pruebas en redes reales

El plan de pruebas por red, módulo y plugin está en `testenv/real/PLAN.md`. Ver [Probar la beta](#beta).

### Los conjuntos de tests

| Conjunto | Dónde | Qué cubre |
| --- | --- | --- |
| Protocolo (16) | `crates/xbnc-proto` | Parser y serializador, escapado de tags, prefijos, *casemapping*, ISUPPORT, CTCP. |
| Unitarios del servicio (159) | `crates/xbncd/src` | Configuración, Argon2id, cifrado y permisos de la clave, base de datos y migraciones (SQLite y PostgreSQL, y actualización de un esquema antiguo), historial y búsqueda, cursores, sesiones, copias, login, limitador de fuerza bruta, enmascarado de logs, codec, CAP, roles, ajustes, TOTP (vectores del RFC 6238), métricas, tokens, cookies, origen, IP tras un proxy, cobertura de OpenAPI, redirección HTTP→HTTPS, catálogo y validación de módulos, globs, menciones, alias, registros, certificados de cliente y de confianza (con un *handshake* TLS real), FiSH, zonas horarias, idiomas (todas las claves usadas existen, español completo, ficheros comprobados y cargados), plugins (permisos, límites, regresiones del *fuzzing*), DCC, cliente HTTP, firmas de versiones, el ayudante de actualización y límites de conexiones, para qué nombres es válido un certificado TLS, y la dirección del panel que se muestra al instalar, y los textos del instalador (completos en inglés y en español, comprobados en cualquier fichero de idioma, y que existe cada clave que usan los scripts). |
| API (13) | `tests/api.rs` | Primera instalación con código de un solo uso, login, sesiones, CSRF, origen, cabeceras, fuerza bruta, redes, canales, historial, exportación, aislamiento entre usuarios, roles, ajustes en caliente, 2FA, panel, OpenAPI, módulos, certificados, notas, idioma, tokens, métricas, alertas, importación y plugins. |
| Extremo a extremo (16) | `tests/e2e.rs` | El bouncer completo contra un servidor IRC simulado (escenarios abajo). |
| Módulos (17 + 12) | `tests/modules.rs`, `tests/modules_v1.rs` | Cada módulo contra el servidor simulado: ausente y resumen, recuperar nick, respuestas al dispositivo que pregunta, eventos, CTCP, canales fijos, alias, op automático, volver tras un kick, vigilar palabras, MOTD y registros; y los módulos de la 1.0: extras IRCv3 solo para los clientes que los piden, login en servicios (X con la IP oculta y Login on Connect), invitación, ignorar y quitar colores, FiSH en los dos sentidos y DH1080, canales desacoplados, canales inundados, recuperar op, Latin-1 y tareas programadas. |
| Comandos `*status` (12) | `tests/status_commands.rs` | Ayuda agrupada, nombres de comandos de otros bouncers, servidores de reserva (añadir, saltar, promover y cambio automático), huellas de confianza, direcciones de salida, canales desactivados y ordenados, búferes, MOTD y avisos a todos, permisos, redes desde IRC y un idioma añadido como fichero. |
| Plugins (2 + 5 + 7) | `tests/plugins.rs`, `tests/plugins_v1.rs`, `tests/official_plugins.rs` | Plugins respondiendo en la red y al usuario, y cada plugin oficial: seen, trivia, greet, factoids, karma, quotes, games, badwords, acceso web solo a sitios aprobados, recordatorios, tell, encuestas, estadísticas, antiflood, dados y respuestas automáticas. |
| Puerto único (2) | `tests/shared_port.rs` | El panel e IRC en un solo puerto TLS: un navegador recibe el panel, un cliente IRC el bouncer, http:// se envía a https:// y a un cliente IRC sin SSL se le pide activarlo. |
| Panel (3) | `tests/panel.rs` | Chat en el panel, menciones e historial de canal, modo desarrollo de plugins y el catálogo de un clic. |
| SSO (3) | `tests/sso.rs` | Passkeys, OpenID Connect y LDAP. |
| Línea de comandos (2) | `tests/cli.rs` | `xbncd info` ejecutado como un proceso real: el panel en la dirección dada al instalar, y ningún error cuando su salida va a `head`. |
| PostgreSQL (1) | `tests/postgres.rs` | Paso de una base de datos SQLite a PostgreSQL (con `XBNC_TEST_POSTGRES`). |
| Servidor IRC real (1) | `tests/real_ircd.rs` | Un xBNC real contra un servidor IRC real (ver abajo). |
| SDK de Rust (3) | `sdk/rust` | Eventos y respuestas del SDK. |

### Escenarios de extremo a extremo

1. Al conectarse, el cliente recibe la bienvenida, el ISUPPORT de la red, sus canales, el tema y la lista de usuarios.
2. Un mensaje recibido con el cliente cerrado se reproduce al reconectar, con su hora; los mensajes propios no se reproducen al mismo dispositivo y una tercera conexión no recibe duplicados.
3. Dos dispositivos conectados a la vez se sincronizan; un dispositivo nuevo recibe el historial reciente.
4. `CHATHISTORY` LATEST, AFTER y errores, `batch`, el `server-time` del servidor conservado, `msgid` y `echo-message`; no se anuncian capacidades no implementadas.
5. Login con SASL PLAIN.
6. Una contraseña incorrecta se rechaza y la IP se bloquea tras tres fallos, incluso con la contraseña correcta.
7. Red inexistente: el error indica las redes disponibles.
8. Caída del servidor: el bouncer avisa, reconecta con esperas crecientes y vuelve a entrar en los canales.
9. `*status`, y `PART` y `JOIN` con clave guardados (la clave se guarda cifrada).
10. Los comandos al conectar se envían antes de entrar en los canales.
11. Las marcas de lectura se comparten entre dispositivos.
12. `soju.im/bouncer-networks` para clientes modernos.
13. Con `/mode $nick +x` en los comandos al conectar, ningún JOIN llega al servidor (canales guardados, los de los comandos, los del propio cliente) hasta que el servidor oculta el host (396).
14. `/msg *status joinnow` entra en los canales retenidos.
15. Una contraseña incorrecta recibe el formato correcto, y tras los fallos la dirección se bloquea indicando los minutos que quedan y dónde desbloquearla.
16. La contraseña de ejemplo («TU\_CONTRASEÑA») se explica y no bloquea la dirección.

### Servidores IRC reales

`tests/real_ircd.rs` conecta un xBNC real a un servidor IRC real (variable `XBNC_IRCD=host:puerto`) y comprueba la lista de usuarios, los mensajes en los dos sentidos, la sincronización entre dos dispositivos, la reproducción sin duplicados con el cliente cerrado, el cambio de nick, salir de un canal y seguir conectado a la red:

```sh
XBNC_IRCD=127.0.0.1:6667 cargo test -p xbncd --test real_ircd -- --ignored
```

El CI lo ejecuta contra Ergo, InspIRCd, UnrealIRCd, Solanum y miniircd (`testenv/ci/start-ircd.sh` compila o instala cada uno con una configuración mínima).

### Fuzzing

`fuzz/` tiene seis objetivos de libFuzzer:

| Objetivo | Qué recibe |
| --- | --- |
| `irc_message` | Líneas IRC arbitrarias: parser, serializador y la vuelta atrás. |
| `dcc` | Ofertas DCC. |
| `import_config` | Ficheros de configuración de otro bouncer. |
| `search_query` | Consultas de búsqueda contra el índice de texto completo. |
| `plugin_manifest` | Ficheros WebAssembly con manifiestos arbitrarios. |
| `plugin_runtime` | Módulos WebAssembly arbitrarios cargados y ejecutados como plugins. |

```sh
cargo install cargo-fuzz          # necesita Rust nightly
cd fuzz && mkdir -p corpus/irc_message && cp seeds/irc_message/* corpus/irc_message/
cargo +nightly fuzz run irc_message corpus/irc_message -- -max_total_time=600
```

El CI (`.github/workflows/fuzz.yml`) ejecuta cada objetivo 20 minutos cada semana, conserva el corpus entre ejecuciones y antes comprueba que los fallos ya encontrados (`fuzz/regressions/`) siguen corregidos. Los fallos encontrados hasta ahora, todos corregidos y con su test:

1. El parser aceptaba CR, LF o NUL dentro de una línea.
2. El parser aceptaba comandos que no son alfanuméricos (`@ @`).
3. Las ofertas DCC aceptaban caracteres de control.
4. Incrustar un manifiesto desbordaba con secciones de tamaño imposible.
5. Un plugin con un bucle largo agotaba la pila nativa del proceso en compilaciones donde el compilador no convierte en saltos las llamadas del intérprete: ahora el intérprete usa el modo de ejecución en bucle, que nunca hace crecer la pila.

### Pruebas de carga

`cargo run --release --example loadtest -- --users 2000 --rate 1 --seconds 60`. Resultados y método en [Rendimiento](PERFORMANCE.md#performance).

### El panel en un navegador

El panel se ha recorrido en Chrome con Playwright contra un bouncer real con un cliente IRC conectado de verdad: primera instalación, asistente de redes, canales, historial y búsqueda, usuarios, seguridad, ajustes, 2FA, módulos, plugins (instalación con la vista previa de permisos, ajustes por red), importación desde otro bouncer, alertas, actualizaciones, copias, chat, la pantalla de entrada, los temas claro y oscuro y el móvil, en inglés y en español, sin errores en la consola salvo el 401 esperado antes de entrar. `testenv/ui/language_check.py URL USUARIO CONTRASEÑA` repite en un navegador real todas las formas de cambiar el idioma (pantalla de entrada, *Tu cuenta*, *Ajustes*, un idioma añadido como fichero) y comprueba que no queda ningún texto en otro idioma.

### Lista de verificación manual

Úsala con `testenv/` o con una instalación real:

- El servicio arranca y para limpiamente (`systemctl start` y `stop xbnc`; en el log aparece *xBNC stopped*).
- Se crean un administrador y varios usuarios con la línea de comandos.
- Un usuario con dos redes está conectado a las dos a la vez.
- Un cliente clásico (HexChat, WeeChat, irssi o mIRC) se conecta con `usuario/red@dispositivo:contraseña`.
- Con el cliente cerrado, otra persona escribe en el canal y por privado; al reconectar esos mensajes llegan una sola vez.
- Dos clientes del mismo usuario ven lo que escribe el otro.
- `docker compose restart ergo` (o cortar la red): xBNC avisa, reconecta y vuelve a los canales.
- `/msg *status status` muestra el estado correcto.
- Cinco contraseñas incorrectas bloquean la IP durante 15 minutos.
- `scripts/backup.sh` y `scripts/restore.sh` recuperan usuarios, redes e historial.
- `journalctl -u xbnc` no contiene contraseñas tras hacer login, SASL y `/msg NickServ IDENTIFY`.
- Desde el panel: crear un usuario, añadirle una red con el asistente, entrar en un canal y ver el historial.
- Un usuario normal no ve Usuarios, Seguridad ni Ajustes, ni las redes de otros.
- Activar la verificación en dos pasos y entrar con el código.
- Cambiar la retención en Ajustes y comprobar en Sistema que se aplica sin reiniciar.
- Consumo de memoria con 1 usuario y 2 redes (`systemctl status xbnc`): anótalo para comparar en versiones futuras.

<a id="test-report"></a>

## Informe para pruebas

En *Tu cuenta → Informe para pruebas → Descargar informe* (o `GET /api/v1/diagnostics`) se descarga un JSON con lo que hace falta para diagnosticar un problema con una red, **sin contraseñas, mensajes, nicks de otras personas ni IPs de tus dispositivos**.

| Campo | Contenido |
| --- | --- |
| `report`, `format` | xbnc-diagnostics, versión 1 del formato. |
| `version`, `brand` | Versión de xBNC y «xBNC 1.0.0 Made by y2k». |
| `os`, `arch`, `database` | Sistema, arquitectura y base de datos (sqlite o postgresql). |
| `started_at`, `generated_at` | Cuándo arrancó xBNC y cuándo se generó el informe. |
| `timezone`, `language` | Tu zona horaria e idioma. |
| `counters` | Contadores del servidor (tráfico, mensajes, reconexiones, logins). |
| `networks[]` | Por red: name, host, port, tls, enabled, si tiene SASL o contraseña de servidor, saved\_channels, modules y plugins activos, y live. |
| `networks[].live` | state, server, network\_name, connected\_since, last\_error, caps\_offered, caps\_acked, isupport, services\_logged\_in, host\_hidden, channels\_joined y clients (dispositivo y capacidades de cada uno). |

<a id="beta"></a>

## Probar la beta

La versión 1.0.0 está en pruebas en redes reales (Undernet, Libera.Chat, freenode, Rizon, EvilNet, IRCnet y otras) antes de declararla estable.

1. Instálala y añade tus redes.
2. Sigue el plan de pruebas de `testenv/real/PLAN.md`: una sección por red, módulo y plugin.
3. Si algo falla, descarga el [informe para pruebas](#test-report) de *Tu cuenta*.
4. Envíalo con lo que hiciste, lo que esperabas, lo que pasó, la red, el cliente, la hora aproximada, la versión (`/msg *status version`) y los avisos del registro (`sudo journalctl -u xbnc --since "1 hour ago" | grep -E "WARN|ERROR"`; revísalo antes de enviarlo).

Si el proyecto está en GitHub, usa la plantilla de incidencia «Informe de prueba». Los fallos de seguridad no se publican: sigue `SECURITY.md`.

### Límites conocidos

- Undernet y QuakeNet no ofrecen TLS: la conexión con esas redes va sin cifrar.
- FiSH es criptografía antigua (Blowfish, sin integridad): protege de los servidores de la red, no sustituye al cifrado de extremo a extremo. Los mensajes descifrados se guardan en tu historial como el resto.
- Algunos clientes no envían comandos que no conocen: los alias necesitan `/quote`.
- DCC entre un cliente solo IPv6 y otro solo IPv4 no se traduce.
- La búsqueda por trozos de palabra solo se usa si no hay coincidencias por palabra completa.
- Si el proceso se corta de golpe, se pueden repetir unos segundos de historial.
- En un idioma añadido como fichero, las respuestas de los plugins oficiales salen en inglés (hablan inglés y español).
- En esta beta la instalación es desde el código; no hay paquetes .deb ni .rpm publicados.
- Pendiente de comprobar en redes reales: login con X, NickServ, Q y AuthServ; invitación y desbaneo; plugins con acceso web; FiSH con clientes reales; instalador con systemd real y Let's Encrypt; DCC entre redes; SASL EXTERNAL; importador con instalaciones reales.
