# Administración

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Línea de comandos](#cli)
- [Alertas y monitorización](#monitoring)
- [Versiones y actualizaciones](#updates)

<a id="cli"></a>

## Línea de comandos

Todos los comandos aceptan `-c RUTA` (o la variable `XBNC_CONFIG`; por defecto `/etc/xbnc/xbnc.toml`), `-h` para la ayuda y `-V` para la versión. Los que escriben en la base de datos se ejecutan como el usuario del servicio: `sudo -u xbnc xbncd …` (en Docker, `docker compose exec xbnc xbncd …`). Después, `sudo systemctl reload xbnc` para que el servicio aplique los cambios de usuarios, redes y canales. Cada acción queda en la auditoría.

| Comando | Qué hace | Opciones |
| --- | --- | --- |
| `xbncd run` | Ejecuta el bouncer en primer plano (lo hace el servicio). SIGHUP (systemctl reload) relee redes, canales, usuarios e idiomas. | — |
| `xbncd init` | Crea la base de datos y la clave secreta. | — |
| `xbncd check-config` | Comprueba el fichero de configuración y termina. | — |
| `xbncd info` | Muestra la dirección del panel y cómo se conectan los clientes. | — |
| `xbncd setup-code` | Muestra el código de un solo uso para crear el primer administrador. | — |
| `xbncd user add NOMBRE` | Crea un usuario. La contraseña se pide dos veces, se lee de la entrada con --password-stdin o de XBNC\_PASSWORD. | --role superadmin\|admin\|moderator\|user\|limited (user por defecto); --admin (atajo de --role superadmin); --password-stdin |
| `xbncd user list` | Lista los usuarios. | — |
| `xbncd user passwd NOMBRE` | Cambia la contraseña. | --password-stdin |
| `xbncd user role NOMBRE ROL` | Cambia el rol. | — |
| `xbncd user enable\|disable NOMBRE` | Reactiva o suspende una cuenta (tras reload, sus redes se desconectan). | — |
| `xbncd user reset-2fa NOMBRE` | Quita la verificación en dos pasos (móvil perdido). | — |
| `xbncd user delete NOMBRE --yes` | Borra el usuario, sus redes y su historial. | --yes (confirmar) |
| `xbncd network add USUARIO NOMBRE --host SERVIDOR` | Añade una red. Las contraseñas SASL y de servidor se piden o se leen de XBNC\_SASL\_PASSWORD y XBNC\_SERVER\_PASSWORD. | --port (6697); --no-tls; --nick; --username; --realname; --server-password; --sasl-user CUENTA |
| `xbncd network list USUARIO` | Lista las redes de un usuario. | — |
| `xbncd network enable\|disable USUARIO NOMBRE` | Activa o desactiva una red. | — |
| `xbncd network delete USUARIO NOMBRE --yes` | Borra una red. | --yes |
| `xbncd channel add USUARIO RED CANAL` | Guarda un canal para entrar automáticamente. | --key (pide la clave del canal) |
| `xbncd channel list USUARIO RED` | Lista los canales guardados. | — |
| `xbncd channel delete USUARIO RED CANAL` | Quita un canal guardado. | — |
| `xbncd plugin install FICHERO.wasm` | Instala o actualiza un plugin; cada usuario lo activa en sus redes. | --yes (aceptar los permisos que pide) |
| `xbncd plugin list` | Lista los plugins instalados. | — |
| `xbncd plugin remove ID` | Desinstala un plugin y borra sus ajustes en todas las redes. | — |
| `xbncd plugin check FICHERO.wasm` | Comprueba un plugin y muestra su manifiesto. | — |
| `xbncd plugin pack PLUGIN.wasm MANIFIESTO.json -o SALIDA.wasm` | Mete el manifiesto (JSON) dentro de un plugin compilado. | -o, --output (obligatoria) |
| `xbncd plugin catalog-keygen -o CLAVE` | Para publicar un catálogo: crea la clave de firma (guárdala bien). | -o, --output |
| `xbncd plugin catalog-entry FICHERO.wasm URL` | Para publicar un catálogo: imprime la entrada del índice de un plugin publicado en URL (https://). | — |
| `xbncd plugin catalog-sign --key CLAVE ÍNDICE` | Para publicar un catálogo: firma el índice (escribe ÍNDICE.sig). | -k, --key |
| `xbncd backup DESTINO` | Copia consistente de la base de datos, con xBNC en marcha. | — |
| `xbncd migrate-to-postgres URL` | Copia la base de datos SQLite a una PostgreSQL nueva y vacía. Para xBNC antes; después pon database.url. | — |
| `xbncd import RUTA` | Trae usuarios, redes, canales y módulos de otro bouncer (su carpeta de datos, con configs/ y users/, o su fichero de configuración). Sin --apply no escribe nada. | --apply; --logs (también los registros); --utc-offset ZONA (+02:00 o Europe/Madrid; +00:00 por defecto) |
| `xbncd import-logs USUARIO RED CARPETA` | Importa registros de texto (una carpeta por canal con ficheros AAAA-MM-DD.log) al historial de una red. Se puede repetir sin duplicar. | --utc-offset ZONA |
| `xbncd audit` | Últimas acciones administrativas. | --limit N (50) |
| `xbncd release keygen` | Para publicar versiones: crea el par de claves de firma. La pública va en \[updates\] public\_key. | --out FICHERO (xbnc-release.key) |
| `xbncd release sign FICHEROS…` | Escribe FICHERO.sig junto a cada paquete. La clave privada se lee de XBNC\_RELEASE\_KEY o de --key. | --key |
| `xbncd release verify --public-key CLAVE FICHEROS…` | Comprueba las firmas .sig. | — |
| `xbncd apply-update` | Instala una actualización pedida desde el panel (lo ejecuta como root el servicio xbnc-update; vuelve a comprobar la firma). | — |
| `xbncd locales check FICHEROS…` | Comprueba ficheros de idioma contra el inglés: textos que faltan, textos que xBNC no usa, {0} o etiquetas \<b\> rotas. No necesita la configuración. | --missing (lista también lo que falta) |
| `xbncd locales list` | Idiomas que ofrece este servidor, con el porcentaje traducido de cada uno. | — |

### Ejemplos

```sh
sudo xbncd info
sudo xbncd setup-code
sudo -u xbnc xbncd user add alicia
sudo -u xbnc xbncd user add carla --role moderator
echo 'una-contraseña-larga' | sudo -u xbnc xbncd user add bob --password-stdin
sudo -u xbnc xbncd user role carla admin
sudo -u xbnc xbncd user reset-2fa alicia
sudo -u xbnc xbncd network add alicia libera --host irc.libera.chat --sasl-user alicia
sudo -u xbnc xbncd network add alicia undernet --host irc.undernet.org --port 6667 --no-tls
sudo -u xbnc xbncd channel add alicia libera '#xbnc' --key
sudo -u xbnc xbncd backup /var/lib/xbnc/copia.db
sudo -u xbnc xbncd import /home/antiguo/.bouncer --logs --utc-offset Europe/Madrid             # vista previa
sudo -u xbnc xbncd import /home/antiguo/.bouncer --logs --utc-offset Europe/Madrid --apply     # importar
sudo systemctl reload xbnc
```

### Scripts

| Script | Uso |
| --- | --- |
| `scripts/install.sh` | Instala o reconfigura el servicio (ver [Instalación](INSTALL.md#install)). |
| `scripts/update.sh [--binary RUTA] [--build]` | Actualiza el binario con copia previa y vuelta atrás automática. |
| `scripts/prereqs.sh [--check]` | Instala lo necesario para compilar xBNC (herramientas y Rust), o con --check solo dice qué falta. |
| `scripts/backup.sh [CARPETA] [--keep N]` | Copia en caliente: base de datos, clave secreta y /etc/xbnc en un .tar.gz 0600 (en /var/backups/xbnc, conserva 14). |
| `scripts/restore.sh ARCHIVO [--with-config] [--yes]` | Restaura una copia de backup.sh. Guarda los ficheros actuales como \*.pre-restore-FECHA. |
| `scripts/uninstall.sh [--purge]` | Quita el servicio y el binario; con --purge, también configuración, datos y el usuario xbnc. |

<a id="monitoring"></a>

## Alertas y monitorización

### Alertas

xBNC comprueba cada 30 segundos estas condiciones. Se ajustan en *Ajustes → Alertas* (0 desactiva cada una):

| Alerta | Se activa cuando | Por defecto |
| --- | --- | --- |
| `network_down` | Una red lleva más de N minutos intentando conectar sin conseguirlo (las que desconectas tú no cuentan). Hasta 10 080 (una semana). | 15 minutos |
| `failed_logins` | Hay N o más accesos fallidos (IRC, panel y tokens) en los últimos 10 minutos. | 30 |
| `memory` | La memoria del proceso supera N MB. | desactivada |
| `database` | La base de datos supera N MB. | desactivada |

Cuando una alerta empieza y cuando se resuelve:

- Aparece (o desaparece) en el **Inicio** del panel para los administradores.
- Queda en la auditoría (acción `alert`).
- Con «Avisar por IRC a los administradores» (activado por defecto), cada administrador y superadministrador conectado recibe un aviso de `*status` en su idioma.
- Con un **webhook** (solo lo configura un superadministrador), xBNC envía un POST con JSON. Tiene 10 segundos para responder con 2xx; si falla, queda en el registro del servicio.

```json
{
  "source": "xbnc",
  "server": "bnc.ejemplo.org",
  "state": "firing",
  "alert": "network_down",
  "subject": "alicia/libera",
  "message": "Network alicia/libera has been disconnected for more than 15 minutes",
  "since": 1762600000000,
  "text": "[xBNC bnc.ejemplo.org] firing: Network alicia/libera ...",
  "content": "[xBNC bnc.ejemplo.org] firing: Network alicia/libera ..."
}
```

`state` es `firing` o `resolved`. Los campos `text` y `content` hacen que funcione tal cual con los webhooks entrantes de Slack y Mattermost (`text`) y de Discord (`content`).

### Métricas para Prometheus

`GET /api/v1/metrics` en formato Prometheus, con la sesión de un administrador o un token de alcance «métricas»:

```yaml
scrape_configs:
  - job_name: xbnc
    scheme: https
    metrics_path: /api/v1/metrics
    authorization:
      credentials: xbnc_...
    static_configs:
      - targets: ["bnc.ejemplo.org:8443"]
```

| Métrica | Tipo | Qué mide |
| --- | --- | --- |
| `xbnc_info` | gauge | Versión del servidor (etiqueta version). |
| `xbnc_uptime_seconds` | gauge | Segundos desde que arrancó. |
| `xbnc_users` | gauge | Cuentas. |
| `xbnc_networks` | gauge | Redes configuradas. |
| `xbnc_networks_running{state}` | gauge | Conexiones por estado: connected, connecting, disconnected, reconnecting. |
| `xbnc_irc_clients` | gauge | Clientes IRC conectados. |
| `xbnc_connections` | gauge | Conexiones IRC abiertas al bouncer, también sin autenticar. |
| `xbnc_blocked_ips` | gauge | Direcciones bloqueadas por fallos. |
| `xbnc_server_bytes_received_total`, `xbnc_server_bytes_sent_total` | counter | Bytes recibidos de y enviados a los servidores IRC. |
| `xbnc_client_bytes_received_total`, `xbnc_client_bytes_sent_total` | counter | Bytes recibidos de y enviados a los clientes. |
| `xbnc_server_lines_total` | counter | Líneas recibidas de los servidores. |
| `xbnc_messages_stored_total` | counter | Mensajes guardados en el historial. |
| `xbnc_reconnects_total` | counter | Reconexiones a servidores IRC. |
| `xbnc_client_logins_total` | counter | Logins correctos de clientes IRC. |
| `xbnc_failed_logins_total` | counter | Logins fallidos (IRC y panel). |
| `xbnc_history_messages` | gauge | Mensajes guardados en el historial. |
| `xbnc_database_bytes` | gauge | Tamaño de la base de datos. |
| `xbnc_resident_memory_bytes` | gauge | Memoria del proceso. |
| `xbnc_log_lines_dropped_total` | counter | Líneas del módulo log descartadas porque el disco no daba abasto. |
| `xbnc_dcc_relays_total` | counter | Envíos y chats DCC pasados por xBNC. |
| `xbnc_dcc_bytes_total` | counter | Bytes de DCC pasados por xBNC. |
| `xbnc_alerts_firing{alert}` | gauge | Alertas activas por tipo. |

Los mismos contadores se ven desde IRC con `/msg *status traffic` (administradores).

### Grafana y reglas de Prometheus

En `packaging/monitoring/`:

- `grafana-dashboard.json`: un panel listo para importar con redes conectadas y sin conectar, clientes IRC, usuarios, memoria, tiempo en marcha, mensajes por segundo, tráfico, redes por estado, logins, reconexiones, almacenamiento, historial y alertas activas.
- `prometheus-rules.yml`: alertas XbncDown, XbncNetworksDisconnected, XbncManyReconnects, XbncFailedLogins, XbncBlockedAddresses, XbncMemoryHigh, XbncDatabaseLarge, XbncLogLinesDropped y XbncInternalAlert.

### Comprobación de vida

`GET /api/v1/health` responde sin sesión; sirve para balanceadores y monitores externos.

<a id="updates"></a>

## Versiones y actualizaciones

### Aviso de versiones nuevas

Con `[updates] repository` (o `url`) en `xbnc.toml`, xBNC consulta una vez al día la última versión publicada y avisa en el Inicio y en *Ajustes → Actualizaciones*. Por sí solo no descarga ni instala nada.

### Actualizar

En esta beta, xBNC se instala desde el .zip: se actualiza descomprimiendo el nuevo y ejecutando `scripts/update.sh` (ver [Instalación](INSTALL.md#install)).

### Actualizar desde el panel (instalaciones con paquete)

xBNC incluye también la actualización con un clic para cuando se instala desde paquetes .deb o .rpm firmados. Con `[updates] public_key`, *Ajustes → Actualizaciones* muestra «Actualizar ahora» a los superadministradores. xBNC descarga el paquete y su firma `.sig`, comprueba la firma Ed25519 y lo deja en `/var/lib/xbnc/updates/`. El servicio del sistema `xbnc-update` lo vuelve a comprobar todo como root sin fiarse de esa carpeta (verifica con la clave de `/etc/xbnc/xbnc.toml`, que xBNC no puede cambiar, y rechaza versiones iguales o más antiguas), lo instala con dpkg o rpm y xBNC se reinicia. Un paquete sin firma, firmado con otra clave o modificado no se instala nunca. Sin `public_key`, o sin instalación por paquete, el panel explica cómo actualizar a mano.

### Publicar versiones firmadas

```sh
# Una vez: crear la clave (la privada, secreta; la pública va en [updates] public_key)
xbncd release keygen --out xbnc-release.key

# En cada versión: firmar los paquetes (escribe FICHERO.sig)
XBNC_RELEASE_KEY="$(cat xbnc-release.key)" xbncd release sign xbnc_1.0.0_amd64.deb xbnc-1.0.0-1.x86_64.rpm

# Comprobar
xbncd release verify --public-key "BASE64..." xbnc_1.0.0_amd64.deb
```

El repositorio incluye un flujo que compila y firma paquetes .deb y .rpm y una imagen de contenedor (`.github/workflows/release.yml`). Necesita la clave como secreto del repositorio (`XBNC_RELEASE_KEY`) y la clave pública como variable (`XBNC_RELEASE_PUBLIC_KEY`). Durante la beta xBNC se publica solo como un .zip con el código fuente, así que el flujo solo se ejecuta cuando se lanza a mano desde la pestaña *Actions*.
