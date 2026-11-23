<a id="troubleshooting"></a>

# Problemas frecuentes

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Empieza siempre por el registro y por `*status`: `sudo journalctl -u xbnc -n 100 --no-pager` y, desde tu cliente, `/msg *status status`.

### Instalación

| Síntoma | Causa probable | Solución |
| --- | --- | --- |
| «La compilación ha fallado» y «SIGKILL» o «Killed» en la salida | La máquina se quedó sin memoria al compilar (necesita unos 2 GB, contando la swap). | Añade swap y vuelve a ejecutar el instalador: sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile |
| «El binario preparado no funciona en este sistema» | El sistema es anterior a 2019 (glibc anterior a 2.28). | No hay que hacer nada: el instalador lo compila solo. Tarda unos minutos. |
| «No se pudo instalar Rust» | El servidor no llega a https://sh.rustup.rs (cortafuegos o proxy). | Permite HTTPS hacia sh.rustup.rs y static.rust-lang.org, o instala Rust a mano y vuelve a ejecutar el instalador. Con el .zip de una versión, en x86\_64, este paso no hace falta: el programa viene ya compilado. |
| «Hacen falta unos 3 GB libres» | No hay disco suficiente para compilar. | Libera espacio, o pon la carpeta xbnc en un disco con más sitio. |
| «Se necesita systemd» | El sistema no usa systemd (Alpine, contenedores). | Usa Docker (ver Docker en Instalación) o una distribución con systemd. |
| Después de git pull y update.sh, la versión sigue siendo la antigua | Sin --build, update.sh toma el programa ya compilado en target/ por la instalación anterior. | Ejecuta sudo ./scripts/update.sh --build. |
| El servicio se reinicia una y otra vez y el registro dice «cannot listen on 0.0.0.0:PUERTO» … «Address already in use» | Otro programa ya usa ese puerto. El instalador comprueba los puertos del panel e IRC, pero no el 6667 sin SSL, que a menudo lo ocupa un servidor IRC en la misma máquina. | Mira qué programa lo usa: sudo ss -ltnp \| grep PUERTO. Después cambia el puerto, por ejemplo sudo ./scripts/install.sh --reconfigure --plain-port 6668 (o --plain-port off), o para el otro programa. |

### No puedes conectar

| Síntoma | Causa probable | Solución |
| --- | --- | --- |
| El cliente dice «certificado no válido» | Instalaste por IP con certificado propio. | Acepta el certificado en el cliente o pasa a dominio: sudo ./scripts/install.sh --reconfigure --domain … --letsencrypt … |
| Usuario o contraseña incorrectos (usuario «…») | Usuario o contraseña incorrectos. | La contraseña del servidor es usuario/red@dispositivo:contraseña, con el usuario y la contraseña del panel. El panel muestra la tuya lista para copiar en Redes → tu red; sudo xbncd info las lista. |
| «TU\_CONTRASEÑA» es el ejemplo | Se copió el ejemplo sin la contraseña. | Escribe tu contraseña después de los dos puntos. No cuenta como intento fallido. |
| Demasiados accesos fallidos desde tu dirección (…): está bloqueada N minutos más | IP bloqueada por fallos, en IRC y en el panel. Un cliente que reconecta solo con una contraseña incorrecta la vuelve a bloquear una y otra vez. | Corrige (o cierra) el cliente y desbloquea la IP en Seguridad → IPs bloqueadas, o espera. |
| No hay ninguna red llamada «…» en tu cuenta | La red de la contraseña del servidor no es una de las tuyas. | Usa el nombre corto de la red tal como lo muestra el panel (el mensaje las lista). |
| Indica qué red usar en la contraseña del servidor | Tienes varias redes y no indicaste cuál. | Añade /red al usuario. |
| Tu cuenta todavía no tiene redes | La cuenta no tiene ninguna red, o está desactivada. | Añade una en Redes → Añadir red, o actívala. |
| Desde casa, el dominio no conecta | Tu router no admite NAT loopback (hairpin NAT). | Desde casa usa la IP local de la máquina de xBNC (Desde tu red local, en el panel). |
| Por la IP local, el cliente rechaza el certificado | El certificado es del dominio (Let's Encrypt), no de la IP. | Desde casa usa el puerto 6667 sin SSL, o activa «aceptar certificados no válidos». |
| this port uses SSL/TLS. Enable SSL… | Tu cliente se conecta sin SSL al puerto que usa SSL (8443 o 6697). | Activa SSL o usa el 6667 desde tu red local. |
| this port only accepts connections from the local network | Usas el 6667 desde fuera. | Usa el puerto con SSL (8443 o 6697). |
| El cliente no conecta y en el registro no aparece nada | El cliente no llega a la máquina (IP, puerto o cortafuegos). | sudo xbncd info muestra el puerto correcto; desde el cliente: openssl s\_client -connect IP:PUERTO (Linux o macOS) o Test-NetConnection IP -Port PUERTO (Windows). |
| Registro: client sent TLS to a plain-text port | El cliente tiene SSL activado contra el 6667. | Desactiva SSL o usa el puerto con SSL. |
| Registro: TLS handshake failed | El cliente no acepta el certificado propio. | Activa «aceptar certificados no válidos» o usa --domain con --letsencrypt. |

### Problemas con una red

| Síntoma | Causa probable | Solución |
| --- | --- | --- |
| \*status: connection failed: … invalid peer certificate | Certificado del servidor IRC no válido (propio o caducado) o faltan los certificados raíz. | sudo apt install ca-certificates; comprueba el host y el puerto TLS. Si la red usa un certificado propio, confía en él con /msg \*status addfingerprint HUELLA. |
| \*status: registration timed out | Puerto equivocado (TLS contra un puerto sin TLS o al revés). | Revisa el puerto y el TLS de la red (Undernet: 6667 sin TLS). |
| SASL authentication failed | Cuenta o contraseña SASL incorrectas. | Corrígelas en la configuración de la red. |
| SASL is configured but this server does not offer SASL PLAIN | La red no admite SASL. | Quita SASL y usa Login en servicios. |
| Your host is trying to (re)connect too fast -- throttled | La red frena las reconexiones seguidas desde tu IP. | Espera; xBNC aguarda al menos 2 minutos. No pulses Conectar varias veces. |
| No entra en los canales: «Tu host sigue sin ocultarse después de … segundos» | Se pidió +x (comandos al conectar o Login en servicios) pero la red no ha ocultado el host, normalmente porque falló el login con X. | Revisa la cuenta y la contraseña de X; xBNC entra en cuanto el host se oculta. /msg \*status joinnow entra ya, con el host real. |
| El login con X, Q o AuthServ no funciona | Cuenta o contraseña del servicio incorrectas. | Revisa Login en servicios; el motivo llega por \*status. |
| En IRCnet no se identifica | IRCnet no tiene NickServ; xBNC no envía la contraseña a propósito. | Usa SASL de sasl.ircnet.com en la red, o nada. |
| El nick termina en «\_» | El nick estaba ocupado al conectar. | Activa Recuperar nick o escribe /nick tu-nick. |
| xBNC cambia mi nick todo el rato | Recuperar nick vuelve al nick guardado. | /msg \*status setnick nuevo, o desactiva el módulo. |
| Veo «�» en lugar de tildes | Raro: xBNC convierte Latin-1 a UTF-8. | Si quien no ve bien las tildes es la otra persona, añade su canal o nick en Codificación. |
| El certificado de cliente no identifica | La huella no está en tu cuenta, la red no usa TLS o no reconectaste. | Añade la huella (SHA-512 en Libera, SHA-1 en OFTC) y reconecta. |

### Módulos y plugins

| Síntoma | Causa probable | Solución |
| --- | --- | --- |
| /msg \*plugin solo muestra «Uso: MSG \<apodo\> \<mensaje\>» | Es la ayuda de tu cliente: no escribiste nada después del nombre. | /msg \*poll ayuda. |
| Un alias no hace nada | Tu cliente no envía comandos que no conoce. | /quote nombre argumentos. |
| No me da op el módulo autoop | No tienes op, la máscara no coincide o la otra persona no responde al desafío. | Comprueba con /whois el usuario@host real y la clave. |
| Un plugin con acceso web no responde | Sus sitios no están aprobados. | Un superadministrador los aprueba en Plugins («Sitios a los que puede conectarse»). |
| Un plugin se ha detenido en una red | Falló cinco veces seguidas. | Mira el registro de Plugins; desactívalo y actívalo de nuevo tras corregirlo. |
| No aparecen ficheros de registro | El módulo log está desactivado o no hay permisos en /var/lib/xbnc/logs. | Actívalo; el registro del servicio avisa si no puede escribir. |
| Las tareas programadas salen a otra hora | Tu zona horaria no está puesta. | Tu cuenta → Zona horaria. |

### Panel y servicio

| Síntoma | Causa probable | Solución |
| --- | --- | --- |
| El panel no carga (conexión rechazada) | Falta \[web\] o escucha en 127.0.0.1. | sudo xbncd info; para rehacer la configuración, sudo ./scripts/install.sh --reconfigure. |
| «La página estaba desactualizada» | Token CSRF antiguo tras volver a entrar. | Recarga la página. |
| «Petición rechazada por venir de otro sitio» | public\_url no coincide con la dirección del navegador. | Corrige public\_url o entra por esa dirección. |
| Tras entrar vuelves a la pantalla de login | Cookie Secure en HTTP sin TLS. | Usa HTTPS o localhost (o secure\_cookies = false solo en pruebas). |
| Todos quedan bloqueados detrás de Nginx | Sin trust\_proxy, todo viene de 127.0.0.1. | trust\_proxy = true y la cabecera X-Forwarded-For en Nginx. |
| secret key file … not found; run xbncd init first | No se inicializó. | sudo -u xbnc xbncd init. |
| secret key … is accessible by other users | Permisos de la clave demasiado abiertos. | sudo chmod 600 /var/lib/xbnc/secret.key && sudo chown xbnc:xbnc /var/lib/xbnc/secret.key. |
| listener … has no TLS certificate | Puerto público sin TLS. | Añade tls\_cert y tls\_key, usa lan\_only o 127.0.0.1. |
| database schema version N is newer than this binary supports | Has vuelto a un binario más antiguo. | Usa el binario nuevo o restaura la copia previa. |
| Los cambios hechos con la terminal no se ven | El servicio no se recargó. | sudo systemctl reload xbnc. |
| Un idioma nuevo no aparece en el panel | El fichero no está en locales\_dir, tiene otro nombre o no se recargó xBNC. | xbncd locales check FICHERO; cópialo en /etc/xbnc/locales/CÓDIGO.json y sudo systemctl reload xbnc. El registro dice qué idiomas cargó. |
| El cliente se desconecta solo con mucho tráfico | La cola del cliente se llenó. | Aumenta client\_queue; al volver recibe lo pendiente. |
| Playback was truncated | Había más de playback\_max mensajes pendientes. | Aumenta playback\_max o usa un cliente con draft/chathistory. |
| Perdiste el código de instalación | — | sudo xbncd setup-code, o crea el usuario con la terminal. |
| Perdiste el móvil con la verificación en dos pasos | — | sudo -u xbnc xbncd user reset-2fa usuario. |
| Olvidaste la contraseña del único administrador | — | sudo -u xbnc xbncd user passwd usuario. |

### Más detalle en el registro

```sh
# En /etc/xbnc/xbnc.toml:  [log] level = "info,xbncd=debug"
sudo systemctl restart xbnc
sudo journalctl -u xbnc -f
sudo journalctl -u xbnc --since "1 hour ago" -p warning
```

Con `trace` se registra cada línea IRC (con las credenciales enmascaradas). Vuelve a `info` al terminar: con `debug` aparecen nicks y canales.
