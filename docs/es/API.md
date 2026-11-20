<a id="api"></a>

# API REST

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Todo lo que hace el panel está en la API. Base: `/api/v1`, en la misma dirección que el panel. Formato JSON; los errores son `{"error": "código", "message": "texto"}` con el código HTTP adecuado. La especificación OpenAPI 3.1 está en `GET /api/v1/openapi.json` (y en `docs/openapi.json`); un test comprueba que documenta todas las rutas.

### Tokens de API

Créalos en *Tu cuenta → Tokens de API*. El token completo (`xbnc_` y 43 caracteres) solo se muestra al crearlo: xBNC guarda su SHA-256. Envíalo en la cabecera `Authorization: Bearer xbnc_…`; con token no hace falta CSRF.

| Alcance | Puede |
| --- | --- |
| `read` | Solo peticiones GET. |
| `write` | Todo lo que puede tu cuenta, salvo cerrar sesión, cambiar la contraseña, la verificación en dos pasos, las passkeys y crear o revocar tokens (eso exige la sesión del panel y responde 403 session\_required). |
| `metrics` | Solo GET /metrics. |

Caducan a los 30, 90 o 365 días, o nunca; máximo 50 por usuario. La lista muestra el prefijo, el último uso y la última IP. Un token incorrecto cuenta como intento fallido (mismos bloqueos que IRC y el panel). Crear y revocar queda en la auditoría.

### Sesión del panel

1. `POST /auth/login` con `{"username", "password"}` (y `totp_code` si la cuenta tiene verificación en dos pasos). La respuesta trae la cookie `xbnc_session` y, en el cuerpo, `csrf`.
2. Envía la cookie en cada petición, y en POST, PUT, PATCH y DELETE la cabecera `X-CSRF-Token` con ese valor (`GET /auth/me` lo devuelve otra vez).
3. Las peticiones con una cabecera `Origin` de otro sitio se rechazan; curl y los scripts no la envían.

### Ejemplos

```sh
B=https://bnc.ejemplo.org:8443/api/v1
T="Authorization: Bearer xbnc_..."

# Redes y su estado
curl -s -H "$T" $B/networks

# Buscar en el historial de la red 1
curl -s -H "$T" "$B/networks/1/history?target=%23xbnc&q=reunión&limit=20"

# Reconectar una red (token write)
curl -s -X POST -H "$T" -H 'Content-Type: application/json' -d '{"action":"jump"}' $B/networks/1/control

# Activar un módulo
curl -s -X PUT -H "$T" -H 'Content-Type: application/json' \
  -d '{"enabled":true,"config":{"delay":60,"summary":true}}' $B/networks/1/modules/away

# Con sesión en lugar de token: crear un usuario y darle una red
curl -s -c jar -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"tu-contraseña"}' $B/auth/login > login.json
CSRF=$(python3 -c 'import json;print(json.load(open("login.json"))["csrf"])')
curl -s -b jar -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' \
  -d '{"username":"bruno","password":"una-clave-larga","role":"user","max_networks":3}' $B/users
curl -s -b jar -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' \
  -d '{"user_id":2,"name":"libera","host":"irc.libera.chat","port":6697,"tls":true,"nick":"bruno",
       "sasl_username":"bruno","sasl_password":"secreto","channels":["#xbnc","#xbnc-dev"]}' $B/networks
```

### Todas las rutas

| Método | Ruta | Qué hace | Quién |
| --- | --- | --- | --- |
| `GET` | `/health` | Comprobación de vida. | sin sesión |
| `GET` | `/i18n` | Idiomas instalados (código, nombre, porcentaje traducido, traductores) y el idioma por defecto del servidor. | sin sesión |
| `GET` | `/i18n/{code}` | Textos del panel en un idioma: texto en inglés → traducción. | sin sesión |
| `GET, POST` | `/setup` | Instalación inicial con el código de un solo uso. | sin sesión |
| `POST` | `/auth/login` | Entrar: {username, password, totp\_code}. Devuelve la cookie xbnc\_session y el campo csrf. | sin sesión |
| `POST` | `/auth/logout` | Cerrar la sesión. | sesión |
| `GET` | `/auth/me` | Tu cuenta, tus permisos y el token CSRF. | sesión o token |
| `GET` | `/auth/methods` | Formas de entrar que ofrece el servidor (passkeys, OpenID Connect, LDAP). | sin sesión |
| `GET` | `/auth/oidc/start` | Redirige al proveedor de OpenID Connect. | sin sesión |
| `GET` | `/auth/oidc/callback` | Vuelta desde el proveedor de OpenID Connect. | sin sesión |
| `POST` | `/auth/passkey/options` | Empieza a entrar con passkey (desafío WebAuthn). | sin sesión |
| `POST` | `/auth/passkey` | Entra con la respuesta de la passkey. | sin sesión |
| `GET` | `/dashboard` | Resumen del Inicio (global para administradores). | sesión o token |
| `GET` | `/presets` | Redes conocidas para el asistente. | sesión o token |
| `GET, POST` | `/networks` | Listar (?all=true: todas, administradores) y crear redes: {user\_id, name, host, port, tls, nick, username, realname, server\_password, sasl\_username, sasl\_password, channels, on\_connect}. | crear: user o superior |
| `GET, PATCH, DELETE` | `/networks/{id}` | Ver con estado en vivo, editar (un secreto omitido se conserva, "" lo borra) y borrar. | dueño o admin |
| `POST` | `/networks/{id}/control` | {action: connect \| disconnect \| jump}. | dueño o admin |
| `GET, POST` | `/networks/{id}/channels` | Canales, con disabled (desactivado) y position (orden de entrada); añadir {name, key}. | dueño o admin |
| `DELETE` | `/networks/{id}/channels/{nombre}` | Quitar un canal (codifica # como %23). | dueño o admin |
| `PUT` | `/networks/{id}/channels/{nombre}/history` | Retención del canal: {retention\_days: null (lo del servidor) \| -1 (no guardar) \| 0 (siempre) \| N días}. | dueño o admin |
| `DELETE` | `/networks/{id}/clients/{cliente}` | Desconectar un dispositivo. | dueño o admin |
| `GET` | `/networks/{id}/conversations` | Conversaciones con su número de mensajes. | dueño o admin |
| `GET, DELETE` | `/networks/{id}/history` | Buscar: target, q, sender, before\_id, from, to, limit (hasta 500), más nuevos primero. DELETE borra (?target= una conversación). | dueño o admin |
| `GET` | `/networks/{id}/history/export` | Descargar: ?format=txt\|json y ?target=. | dueño o admin |
| `GET` | `/networks/{id}/chat` | WebSocket del chat del panel: el panel como un cliente IRC más. Mensajes {line: "PRIVMSG #xbnc :hola"} en los dos sentidos. | solo el dueño, sesión |
| `GET, PUT` | `/networks/{id}/on-connect` | Comandos al conectar: {commands: "/msg X login …"}; vacío los borra. | dueño o admin |
| `GET, PUT, DELETE` | `/networks/{id}/cert` | Certificado de cliente: huellas SHA-1/256/512; subir {certificate, private\_key} en PEM; borrar. La clave nunca se devuelve. | dueño o admin |
| `POST` | `/networks/{id}/cert/generate` | Generar un certificado nuevo. | dueño o admin |
| `GET` | `/modules` | Catálogo de módulos: ids, ajustes con tipo y límites. | sesión o token |
| `GET` | `/networks/{id}/modules` | Estado de los módulos de una red (secretos vacíos y secrets\_set). | dueño o admin |
| `PUT` | `/networks/{id}/modules/{módulo}` | {enabled, config}: se valida y se aplica al momento. Un secreto vacío conserva el guardado; null lo borra. | dueño o admin |
| `GET` | `/networks/{id}/plugins` | Plugins de una red. | dueño o admin |
| `PUT` | `/networks/{id}/plugins/{plugin}` | {enabled, config}. | dueño o admin |
| `GET` | `/mentions` | Mensajes que te mencionan, más nuevos primero (?before=ms&limit=). | sesión o token |
| `GET` | `/notes` | Tus notas. | sesión o token |
| `PUT, DELETE` | `/notes/{clave}` | Guardar {text} (hasta 400 caracteres) o borrar una nota. | sesión o token |
| `PUT` | `/me/language` | {language: código de un idioma instalado (en, es…) \| ""} (vacío: el del servidor). | sesión o token |
| `PUT` | `/me/timezone` | {timezone: "Europe/Madrid"} (nombre IANA; vacío: UTC). | sesión o token |
| `PUT` | `/me/password` | Cambiar tu contraseña (cierra tus otras sesiones). | solo sesión |
| `PUT` | `/me/avatar` | Foto de perfil: los bytes de la imagen con Content-Type image/png, image/jpeg o image/webp (como mucho 256 KB y 1024×1024 píxeles; se comprueba el formato real). | sesión o token |
| `DELETE` | `/me/avatar` | Quitar tu foto de perfil. | sesión o token |
| `GET` | `/users/{id}/avatar` | La foto de un usuario: la tuya, o la de cualquiera con view\_users. La dirección con ?v= está en el campo avatar de los usuarios y de /auth/me. | sesión o token |
| `DELETE` | `/users/{id}/avatar` | Quitar la foto de un usuario (manage\_users, en roles por debajo del tuyo). | sesión o token |
| `POST` | `/me/totp/setup` | Empezar a activar la verificación en dos pasos (secreto y QR). | solo sesión |
| `POST` | `/me/totp/enable` | Activarla con un código. | solo sesión |
| `POST` | `/me/totp/disable` | Desactivarla. | solo sesión |
| `GET, POST` | `/me/passkeys` | Tus passkeys; registrar una. | solo sesión |
| `POST` | `/me/passkeys/options` | Empezar a registrar una passkey. | solo sesión |
| `DELETE` | `/me/passkeys/{id}` | Quitar una passkey. | solo sesión |
| `GET, DELETE` | `/me/identities` | Cuentas externas vinculadas (OpenID Connect, LDAP); desvincular. | solo sesión |
| `POST` | `/me/identities/oidc` | Empezar a vincular tu cuenta de OpenID Connect. | solo sesión |
| `GET, PUT` | `/me/notifications` | Ajustes de avisos (el token de Telegram se devuelve oculto). | sesión o token |
| `POST` | `/me/notifications/test` | Enviar un aviso de prueba a todos los canales. | sesión o token |
| `GET, POST, DELETE` | `/me/push` | Dispositivos con push; suscribir {endpoint, keys: {p256dh, auth}, label}; quitar {endpoint}. | sesión o token |
| `GET` | `/push/key` | Clave pública VAPID para Web Push. | sesión o token |
| `GET` | `/diagnostics` | Informe para pruebas: qué admite cada una de tus redes y cómo la ha tratado xBNC (sin secretos ni mensajes). | sesión o token |
| `GET, POST` | `/tokens` | Tus tokens; crear {name, scope: read \| write \| metrics, expires\_days: 0–3650} (0: no caduca). El token solo viene en la respuesta. | solo sesión |
| `DELETE` | `/tokens/{id}` | Revocar un token. | solo sesión |
| `GET` | `/sessions` | Sesiones del panel (?all=true: todas, administradores). | sesión |
| `DELETE` | `/sessions/{id}` | Cerrar una sesión. | sesión |
| `GET, POST` | `/users` | Listar y crear {username, password, role, max\_networks, max\_clients}. | ver: moderator; crear: admin |
| `GET, PATCH, DELETE` | `/users/{id}` | Ver, cambiar {role, enabled, max\_networks, max\_clients, max\_history\_mb} (null quita el límite) y borrar. | según el rol |
| `POST` | `/users/{id}/password` | Restablecer la contraseña (cierra sus sesiones). | admin |
| `GET` | `/users/{id}/usage` | Redes, clientes y espacio de historial usados frente a sus cuotas. | moderator |
| `GET` | `/security` | Accesos e IPs bloqueadas. | moderator |
| `DELETE` | `/security/blocked/{ip}` | Desbloquear una IP. | admin |
| `GET` | `/audit` | Registro de auditoría. | moderator |
| `GET, PUT` | `/settings` | Ajustes en caliente. | ver: admin; editar: superadmin |
| `GET` | `/system` | Versión, base de datos, escuchas y contadores. | admin |
| `POST` | `/system/restart` | Reiniciar xBNC (el servicio lo vuelve a arrancar). | superadmin |
| `GET` | `/alerts` | Alertas activas. | admin |
| `GET` | `/metrics` | Métricas en formato Prometheus. | admin o token metrics |
| `GET` | `/ws` | WebSocket del Inicio: un mensaje {type: "stats"} cada 2 segundos con el estado de las redes y, para administradores, memoria, CPU, clientes, conexiones, IPs bloqueadas y contadores; {type: "logout"} si la sesión termina. | solo sesión |
| `GET, POST` | `/backups` | Copias, restauración preparada y ajustes de las automáticas; crear una copia ahora. | superadmin |
| `GET, DELETE` | `/backups/{nombre}` | Descargar o borrar una copia. | superadmin |
| `POST` | `/backups/{nombre}/restore` | Preparar la restauración de una copia del servidor. | superadmin |
| `POST, DELETE` | `/backups/restore` | Subir una copia (.tar.gz) y prepararla; cancelar la restauración preparada. | superadmin |
| `GET` | `/plugins` | Plugins instalados. | sesión o token |
| `POST` | `/plugins` | Instalar o actualizar: el cuerpo es el .wasm (application/wasm, hasta 4 MB). | superadmin, sesión |
| `POST` | `/plugins/inspect` | Comprobar un .wasm sin instalarlo. | superadmin, sesión |
| `PATCH, DELETE` | `/plugins/{id}` | {enabled} en el servidor y dominios aprobados; desinstalar. | superadmin, sesión |
| `GET` | `/plugins/catalog` | Catálogo: plugins oficiales y catálogo remoto firmado. | sesión o token |
| `POST` | `/plugins/catalog/{id}` | Instalar del catálogo con un clic. | superadmin, sesión |
| `GET, PUT` | `/plugins/dev` | Modo desarrollo: estado y carpeta; activar o desactivar. | superadmin, sesión |
| `GET` | `/plugins/logs` | WebSocket con el registro de los plugins: últimas líneas y las nuevas. | superadmin, sesión |
| `GET` | `/updates` | Versión instalada y última publicada, si se puede actualizar desde el panel y el resultado de la última actualización. | admin |
| `POST` | `/updates/check` | Comprobar ahora si hay versión nueva. | admin, sesión |
| `POST` | `/updates/install` | Descargar, verificar e instalar el paquete firmado de la última versión. | superadmin, sesión |
| `POST` | `/import/preview` | Vista previa de la importación: {config: "contenido del fichero"}. No escribe nada. | superadmin, sesión |
| `POST` | `/import/apply` | Importar. | superadmin, sesión |

«dueño o admin»: el dueño de la red o un administrador. «solo sesión»: no se admite con token. Las contraseñas nunca se devuelven: las redes indican `has_sasl_password` y `has_server_password`.
