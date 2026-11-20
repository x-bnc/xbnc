<a id="data"></a>

# Copias de seguridad, bases de datos y migración

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

### Copias desde el panel

En *Ajustes → Copias de seguridad* (superadministradores): crear una copia ahora, descargarla, borrarla, restaurarla y programar copias automáticas (cada N horas, conservando las últimas M). Cada copia es un `.tar.gz` con la base de datos (copia en caliente con comprobación de integridad), la clave secreta y los plugins instalados, guardado en `/var/lib/xbnc/backups/`.

Restaurar deja la copia preparada (también se puede subir una desde el ordenador) y se aplica cuando xBNC se reinicia; mientras tanto se puede cancelar. Las copias contienen la clave secreta: guárdalas cifradas.

### Copias desde la terminal

```sh
sudo ./scripts/backup.sh                       # en /var/backups/xbnc, conserva 14
sudo ./scripts/backup.sh /mnt/copias --keep 30

# Cada noche a las 3:30
echo '30 3 * * * root /ruta/a/xbnc/scripts/backup.sh >/dev/null' | sudo tee /etc/cron.d/xbnc-backup

# Restaurar (para el servicio, guarda lo actual como *.pre-restore-FECHA, restaura y arranca)
sudo ./scripts/restore.sh /var/backups/xbnc/xbnc-20261108T033000Z.tar.gz
sudo ./scripts/restore.sh ARCHIVO --with-config    # también /etc/xbnc

# Solo la base de datos
sudo -u xbnc xbncd backup /var/lib/xbnc/copia.db
```

### PostgreSQL

```sh
sudo systemctl stop xbnc
sudo -u xbnc xbncd migrate-to-postgres postgres://xbnc:clave@127.0.0.1/xbnc
# en /etc/xbnc/xbnc.toml:
#   [database]
#   url = "postgres://xbnc:clave@127.0.0.1/xbnc"
sudo systemctl start xbnc
```

La base de datos de destino tiene que estar vacía. Con `?sslmode=require` la conexión va cifrada. La búsqueda de texto completo funciona igual con SQLite y con PostgreSQL.

### Alta disponibilidad

Con `ha = true`, varios servidores ejecutan xBNC contra la misma base de datos PostgreSQL. El que tiene un bloqueo de PostgreSQL trabaja y los demás esperan. Si el activo cae, su conexión se cierra, el bloqueo se libera y otro toma el relevo en segundos. Un nodo activo que pierde la base de datos se detiene al momento, así que nunca hay dos a la vez. Los clientes tienen que poder llegar al nodo activo (una IP flotante o un DNS que cambie).

### Migrar desde otro bouncer

xBNC trae lo que tenías en tu bouncer anterior si usa el formato clásico de configuración por bloques (`<User nombre>`, `<Network nombre>`, `<Chan #canal>`).

| Qué | Cómo queda en xBNC |
| --- | --- |
| Usuarios | Con su nombre. Los administradores pasan a admin (nunca a superadmin). Los usuarios que ya existen en xBNC no se tocan. |
| Contraseñas | Las guardadas con SHA-256 con sal siguen funcionando y pasan a Argon2id la primera vez que cada usuario entra. Las de otro formato (MD5) reciben una contraseña nueva aleatoria, que se muestra una sola vez. |
| Redes | Servidor, puerto, TLS, contraseña de servidor, nick, ident y nombre real. Las redes desactivadas siguen desactivadas. Si había varios servidores, se usa el primero (añade los demás después como servidores de reserva con `/msg *status addserver`). Se admiten servidores IPv6. |
| Canales | Con sus claves (cifradas). Los desactivados no se importan. |
| SASL, NickServ y comandos al conectar | SASL pasa a la red; la contraseña de NickServ y los comandos al conectar, a «Comandos al conectar» (cifrados). Necesitan la carpeta de datos. |
| Certificado de cliente | Se importa si está en la carpeta de datos y su clave coincide. |
| Módulos | Se activa el equivalente de xBNC (tabla de abajo). |
| Registros | Los ficheros de texto diarios pasan al historial de cada red, con búsqueda. Se pueden repetir sin duplicar. |

**Desde el panel** (*Usuarios → Importar desde otro bouncer*, superadministradores): elige el fichero .conf o pega su contenido, pulsa «Ver qué se importa» (no escribe nada) y después «Importar». Desde el panel solo se lee la configuración; SASL, NickServ, comandos al conectar, certificados y registros están en la carpeta de datos y se importan desde el servidor.

```sh
# Vista previa: no cambia nada
sudo -u xbnc xbncd import /ruta/a/la/carpeta
# Importar, con los registros (que estaban en hora de Madrid)
sudo -u xbnc xbncd import /ruta/a/la/carpeta --apply --logs --utc-offset Europe/Madrid
sudo systemctl reload xbnc

# Registros de una red concreta, aparte
sudo -u xbnc xbncd import-logs alicia libera /ruta/a/logs/libera --utc-offset America/New_York
```

La carpeta de registros tiene una subcarpeta por canal o conversación (`#xbnc/`, `bob/`) con ficheros `AAAA-MM-DD.log`. Se reconocen mensajes, acciones (`* nick …`), avisos (`-nick- …`) y entradas, salidas, desconexiones, kicks, cambios de nick, temas y modos. Con una zona horaria con nombre (Europe/Madrid) se aplica el horario de verano de cada día.

| Módulo del bouncer anterior | En xBNC |
| --- | --- |
| `keepnick` | Recuperar nick |
| `simple_away`, `away`, `autoaway`, `awaynick` | Ausente automático (con el retraso y el mensaje de simple\_away) |
| `autoop` | Op y voz automáticos (vuelve a añadir las entradas: la sintaxis es otra) |
| `kickrejoin` | Volver tras un kick (con su retraso) |
| `stickychan` | Canales fijos (con la lista) |
| `buffextras` | Eventos en el historial |
| `log` | Registros en ficheros |
| `route_replies` | Respuestas a quien pregunta |
| `cert` | Certificado de cliente |
| `alias`, `watch` | Alias y Vigilar palabras (vuelve a añadir las entradas) |
| `block_motd` | Ocultar el MOTD |
| `ctcpreplies` | Respuestas CTCP (con la respuesta a VERSION) |
| `perform`, `nickserv`, `sasl` | Comandos al conectar y SASL de la red |
| `chansaver`, `savebuff`, `playback`, `clientbuffer`, `webadmin`, `controlpanel`, `fail2ban`, `lastseen`, `notes`, `clearbufferonmsg`, `simple_disconnect`, `missingmotd`, `certauth`, `listsockets`, `identfile`, `partyline` | Ya incluidos en xBNC, siempre activos |

Cualquier otro módulo aparece en la vista previa como «sin equivalente». Después de importar: las redes con varios servidores usan solo el primero; las redes sin TLS aparecen avisadas; si usabas los mismos puertos, para el bouncer anterior antes de arrancar xBNC; y configura tus clientes con el formato `usuario/red`.
