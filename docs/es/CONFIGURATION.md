<a id="configuration"></a>

# Configuración (xbnc.toml)

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

`/etc/xbnc/xbnc.toml` solo guarda ajustes del proceso: usuarios, redes y canales están en la base de datos. El instalador lo crea por ti. Tras editarlo: `sudo xbncd check-config` y `sudo systemctl restart xbnc`. Una clave desconocida es un error, para que una errata no pase desapercibida. Lo de *Ajustes* en el panel tiene prioridad sobre `[history]` y `[limits]`.

### General

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `hostname` | xbnc | Nombre del servidor que ven los clientes. |
| `language` | en | Idioma por defecto de los mensajes de IRC y del panel: en, es o cualquier idioma instalado en locales\_dir. Cada usuario puede elegir el suyo. El instalador pone el que elijas. |
| `locales_dir` | /etc/xbnc/locales | Carpeta de idiomas adicionales y correcciones (un fichero \<código\>.json por idioma, ver [Traducir xBNC](TRANSLATIONS.md#translations)). Se lee al arrancar y al recargar. La variable de entorno XBNC\_LOCALES\_DIR tiene prioridad. |
| `secret_key_file` | obligatoria | Ruta de la clave de 32 bytes que cifra las credenciales. La crea xbncd init con permisos 0600. **Haz copia**: sin ella no se pueden descifrar. |

### \[database\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `path` | obligatoria | Fichero SQLite. Su carpeta es también la de datos (plugins, registros, copias). |
| `url` | — | PostgreSQL en lugar de SQLite: postgres://usuario:clave@host/base. Añade ?sslmode=require para cifrar la conexión. |
| `ha` | false | Alta disponibilidad (necesita url): varios nodos comparten la base de datos; uno trabaja y los demás esperan para tomar el relevo. |

### \[\[listen\]\] (uno por puerto)

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `address` | obligatoria | Dirección y puerto: 0.0.0.0:6697 o \[::\]:6697. |
| `tls_cert` | — | Cadena del certificado (PEM). Obligatoria salvo insecure, lan\_only o una dirección de loopback. |
| `tls_key` | — | Clave privada (PEM). |
| `insecure` | false | Permitir texto plano en una dirección pública (desaconsejado). |
| `lan_only` | false | Solo clientes de la red local (loopback, privadas, enlace local, CGNAT/Tailscale, IPv6 ULA). El resto recibe un ERROR explicativo. Puede ir sin TLS. |

### \[web\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `address` | obligatoria | Dirección del panel y la API, por ejemplo 0.0.0.0:8443 o 127.0.0.1:8080. Sin \[web\], no hay panel. |
| `tls_cert, tls_key` | — | Certificado del panel. |
| `insecure` | false | HTTP sin cifrar en una dirección pública (solo detrás de un proxy TLS en una red de confianza). |
| `secure_cookies` | true con TLS | Marca la cookie de sesión como Secure. Ponlo a true detrás de un proxy TLS. |
| `public_url` | — | URL pública del panel (https://bnc.ejemplo.org). Se usa para comprobar el Origin de las peticiones y como base de OpenID Connect. |
| `session_hours` | 12 | Duración de la sesión en horas (deslizante). |
| `trust_proxy` | false | Fiarse de X-Forwarded-For y X-Real-IP de un proxy en loopback, para que el bloqueo use la IP real. |
| `shared` | false | Servir el panel en el puerto IRC de address (una de las entradas \[\[listen\]\], con TLS): los navegadores ven el panel y los clientes IRC el bouncer, en el mismo puerto, con el certificado del listener (no pongas aquí tls\_cert ni tls\_key). |

### \[history\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `retention_days` | 30 | Borrar mensajes de más de N días. 0, para siempre. |
| `playback_max` | 1000 | Máximo de mensajes reproducidos al reconectar. |
| `new_client_playback` | 100 | Mensajes para un dispositivo que nunca se había conectado. |
| `chathistory_max` | 500 | Máximo limit aceptado en CHATHISTORY. |

### \[limits\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `max_connections` | 1000 | Conexiones IRC de clientes en total. |
| `max_clients_per_user` | 10 | Dispositivos conectados por usuario. |
| `max_networks_per_user` | 10 | Redes por usuario. |
| `login_max_failures` | 5 | Fallos antes de bloquear una IP. |
| `login_window_seconds` | 600 | Ventana en la que se cuentan los fallos. |
| `login_block_seconds` | 900 | Duración del bloqueo. |
| `registration_timeout_seconds` | 30 | Tiempo para completar el login de un cliente. |
| `client_queue` | 4096 | Líneas en cola por cliente antes de desconectarlo por lento (al volver recupera lo pendiente). |

### \[log\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `level` | info | Filtro: error, warn, info, debug, trace, o por módulo (info,xbncd=debug). La variable RUST\_LOG tiene prioridad. Las credenciales se enmascaran en todos los niveles. |

### \[dcc\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `public_ip` | la que usa xBNC hacia la red | Dirección que se anuncia en IRC. Necesaria detrás de NAT. Si la conexión con la red es IPv6, se anuncia la IPv4 pública. |
| `ports` | cualquiera libre | Rango para los túneles, por ejemplo 50000-50100 (ábrelo en el cortafuegos y el router). El instalador escribe 50000-50100 y lo abre. |

### \[smtp\] (avisos por correo)

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `host` | obligatoria | Servidor de correo. |
| `port` | 587 | Puerto. |
| `security` | starttls | starttls, tls (puerto 465) o none (solo relés locales). |
| `username, password` | — | Credenciales. |
| `from` | obligatoria | Remitente, por ejemplo xBNC \<xbnc@ejemplo.org\>. |

### \[oidc\] (OpenID Connect)

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `name` | — | Texto del botón (Google, Keycloak…). |
| `issuer` | obligatoria | URL del emisor; se lee su /.well-known/openid-configuration. |
| `client_id, client_secret` | obligatorias | Datos de la aplicación registrada en el proveedor. La dirección de retorno es `https://TU-PANEL/api/v1/auth/oidc/callback` (usa public\_url). |
| `scopes` | openid profile email | Alcances que se piden. |
| `username_claim` | preferred\_username | Dato que se usa como nombre de las cuentas creadas automáticamente. |
| `auto_create` | false | Crear la cuenta de xBNC la primera vez que alguien entra. |
| `link_existing` | false | Vincular con una cuenta local del mismo nombre (solo con proveedores que controles). |
| `default_role` | user | Rol de las cuentas creadas. |
| `allowed_groups` | \[\] (todos) | Si se indica, solo pueden entrar miembros de alguno de estos grupos. |
| `groups_claim` | groups | Dato con los grupos. |

### \[ldap\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `url` | obligatoria | ldaps://host:636, o ldap://host:389 con starttls = true. |
| `starttls` | false | Usar STARTTLS. |
| `bind_dn` | obligatoria | DN con el que se entra; {{username}} se cambia por el nombre escrito. |
| `ca_file` | certificados del sistema | Fichero PEM con la autoridad del servidor LDAP. |
| `auto_create, link_existing, default_role` | false, false, user | Igual que en \[oidc\]. |

### \[updates\]

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `check` | true | Comprobar una vez al día si hay versión nueva (y cuando un administrador lo pide). No descarga nada. |
| `repository` | — | Repositorio de GitHub con las versiones (dueño/nombre). |
| `url` | — | O cualquier URL https que responda {{"version", "url", "notes", "packages"}}. |
| `public_key` | — | Clave pública Ed25519 de las versiones. Con ella, y xBNC instalado desde paquete, un superadministrador puede actualizar desde el panel. |

### \[catalog\] (catálogo remoto de plugins)

| Clave | Por defecto | Qué hace |
| --- | --- | --- |
| `url` | — | Dirección https:// del índice JSON. |
| `public_key` | — | Clave pública Ed25519 (base64) que firma el índice. |

### Ejemplo completo

```toml
hostname = "bnc.ejemplo.org"
language = "es"
secret_key_file = "/var/lib/xbnc/secret.key"

[database]
path = "/var/lib/xbnc/xbnc.db"
# url = "postgres://xbnc:clave@127.0.0.1/xbnc?sslmode=require"
# ha = true

[[listen]]
address = "0.0.0.0:6697"
tls_cert = "/etc/xbnc/tls/fullchain.pem"
tls_key = "/etc/xbnc/tls/privkey.pem"

[[listen]]
address = "0.0.0.0:6667"
lan_only = true

[web]
address = "0.0.0.0:8443"
tls_cert = "/etc/xbnc/tls/fullchain.pem"
tls_key = "/etc/xbnc/tls/privkey.pem"
public_url = "https://bnc.ejemplo.org:8443"
session_hours = 12

[history]
retention_days = 30
playback_max = 1000
new_client_playback = 100
chathistory_max = 500

[limits]
max_connections = 1000
max_clients_per_user = 10
max_networks_per_user = 10
login_max_failures = 5
login_window_seconds = 600
login_block_seconds = 900
registration_timeout_seconds = 30
client_queue = 4096

[log]
level = "info"

[dcc]
public_ip = "203.0.113.10"
ports = "50000-50100"

[smtp]
host = "smtp.ejemplo.org"
port = 587
security = "starttls"
username = "xbnc@ejemplo.org"
password = "..."
from = "xBNC <xbnc@ejemplo.org>"

[oidc]
name = "Keycloak"
issuer = "https://sso.ejemplo.org/realms/casa"
client_id = "xbnc"
client_secret = "..."
auto_create = true
default_role = "user"
allowed_groups = ["irc"]

[ldap]
url = "ldaps://ldap.ejemplo.org:636"
bind_dn = "uid={username},ou=people,dc=ejemplo,dc=org"

[updates]
repository = "dueño/xbnc"

[catalog]
url = "https://plugins.ejemplo.org/index.json"
public_key = "BASE64..."
```
