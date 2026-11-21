# Novedades

Traducción al español de [CHANGELOG.md](CHANGELOG.md). Las versiones anteriores a la 1.0.0 fueron de desarrollo y no se publicaron; su historial está en `CHANGELOG.md` (en inglés).

## [1.0.0] - 2026-11-21

Primera versión completa, lista para probar en redes reales (`testenv/real/PLAN.md`).

### Añadido

- Un solo puerto para el panel e IRC (`[web] shared = true`, en el instalador `--single-port N`): los navegadores ven el panel y los clientes IRC el bouncer, con el mismo certificado; `http://` se envía a `https://`, y a un cliente IRC sin SSL se le pide que lo active.
- El instalador pregunta los puertos (uno para todo, o uno para el panel y otro para IRC), rechaza los que los navegadores no abren (6667, 6697…) y los que ya están en uso, y rechaza las opciones de puertos que una configuración existente no aplicaría (sin `--reconfigure`).
- Puertos de DCC: el instalador escribe `[dcc] ports = "50000-50100"` y los abre en el cortafuegos (`--dcc-ports A-B|off`).
- Let's Encrypt por DNS para máquinas sin el puerto 80 (`--dns cloudflare|digitalocean|ovh|linode|rfc2136 --dns-credentials FICHERO`, o preguntado en el instalador).
- Foto de perfil en *Tu cuenta*: se recorta a 256×256 en el navegador y xBNC la guarda después de comprobar su formato real (PNG, JPEG o WebP, como mucho 256 KB y 1024×1024); la ven en el panel el usuario y los administradores (`PUT/DELETE /api/v1/me/avatar`, `GET/DELETE /api/v1/users/{id}/avatar`).
- La versión aparece en todas partes como `xBNC v1.0.0 - https://xbnc.org` (panel, página de acceso, bienvenida de IRC, `/msg *status version`), tomada de la versión del código: también es la respuesta CTCP VERSION por defecto (se puede cambiar en el módulo CTCP; `{version}` se sustituye por la versión en marcha) y el mensaje de QUIT que envía xBNC cuando deja una red (desconexión, reconexión, red desactivada, apagado).
- Catálogo oficial firmado de plugins: cuando su clave está puesta, xBNC lo consulta una vez al día y actualiza los plugins instalados que tengan allí una versión más nueva, después de comprobar la firma Ed25519 y cada SHA-256.
- Instalación sin instalar nada antes: el .zip trae el programa ya preparado para x86_64 en `bin/` (funciona en cualquier distribución desde 2019, glibc 2.28 o posterior, y se comprueba con `bin/SHA256SUMS` antes de usarlo). En otras máquinas, o con `--build`, el instalador ejecuta el nuevo `scripts/prereqs.sh`, que instala las herramientas de compilación con el gestor de paquetes del sistema (apt, dnf, yum, pacman, zypper o apk) y Rust con rustup, y compila el programa como dueño de la carpeta del código, nunca como root. Avisa si no hay memoria o disco suficientes para compilar. `update.sh` obtiene el programa de la misma forma. `prereqs.sh --check` dice lo que falta sin instalar nada.
- El instalador habla el idioma elegido para la instalación, y el inglés es el predeterminado. En una terminal lo pregunta primero, con un menú de los ficheros de idioma (`locales/*.json`); `--language CÓDIGO` lo elige sin preguntar. Ese idioma pasa a ser el predeterminado de xBNC, y un idioma que no viene incluido se instala en `/etc/xbnc/locales`. Los textos del instalador, de `prereqs.sh` y de `update.sh` están en una sección nueva `installer` de los ficheros de idioma, así que traducir xBNC a un idioma traduce también su instalador. Se leen solo con bash y awk, y `xbncd locales check` los comprueba como el resto de textos.
- El logo oficial en el panel (página de acceso, menú lateral y cabecera del móvil, con letras oscuras en el tema claro y blancas en el oscuro), como icono del panel, de la pestaña del navegador y de la app instalada en el móvil, en la wiki, en la web y en el README.
- La documentación en español en `docs/es/`, generada desde la wiki en español como la inglesa en `docs/`.
- `/msg *status joinnow`: entra en los canales en espera al momento, sin esperar al host oculto.
- Diez módulos nuevos:
  - Login en servicios: X de Undernet con Login on Connect, NickServ, Q y AuthServ, esperando al host oculto antes de entrar en los canales, y recuperación del nick.
  - Invitación y desbaneo con X, ChanServ o Q.
  - Codificación de caracteres (Latin-1 y Windows-1252).
  - Canales desacoplados (`/quote DETACH #canal`).
  - Ocultar canales con inundación.
  - Quitar colores.
  - Recuperar el op cuando te quedas solo.
  - Cifrado FiSH (ECB y CBC) con intercambio de claves DH1080 (`/msg *crypt`).
  - Ignorar, con SILENCE en Undernet (`/msg *ignore`).
  - Tareas programadas en tu zona horaria.
- Diez plugins oficiales nuevos: trivial, saludos, títulos de enlaces, factoides, el tiempo, noticias RSS/Atom, karma, citas, palabras prohibidas y juegos (ahorcado, bola 8 mágica y piedra, papel o tijera).
- IRCv3 con el servidor: `multi-prefix`, `extended-join`, `away-notify`, `account-notify`, `account-tag`, `chghost`, `userhost-in-names`, `invite-notify`, `setname` y `cap-notify`. También se ofrecen a los clientes, y los que no los piden reciben los mensajes de siempre.
- Zona horaria por usuario (Tu cuenta): ficheros de log y tareas programadas en tu hora local.
- Informe de pruebas (Tu cuenta y `GET /api/v1/diagnostics`): capacidades, ISUPPORT y estado del login de cada red, sin contraseñas ni mensajes.
- Política de seguridad (`SECURITY.md`) y plan de pruebas en redes reales (`testenv/real/PLAN.md`).
- Documentación completa en inglés en `docs/` (instalación, clientes, redes, comandos, panel, módulos, plugins, seguridad, configuración, administración, datos, API, arquitectura, modelo de amenazas, rendimiento, pruebas y problemas frecuentes), generada desde la wiki en inglés, más `docs/STATUS.md`. README, CONTRIBUTING, SECURITY y el plan de pruebas en inglés, y una plantilla de «Informe de pruebas» para las issues de GitHub.
- freenode y EvilNet en el asistente de redes. Undernet usa el puerto 6667 sin TLS, porque la red no lo ofrece.
- Los ajustes secretos de los módulos (contraseñas y claves) se guardan cifrados y el panel no los vuelve a mostrar.
- Los plugins pueden pedir eventos de temporizador cada 5 a 60 segundos (`timer_secs`).
- `*status` completo, con 64 comandos:
  - Ayuda agrupada en secciones, con los nombres de los comandos en negrita y columnas alineadas, en inglés y español. Solo muestra lo que tu cuenta puede usar. `help <comando>` explica uno con un ejemplo, y `help <palabra>` busca.
  - También se aceptan los nombres que usan otros bouncers (`ListChans`, `ListMods`, `AddTrustedServerFingerprint`…), sin distinguir mayúsculas.
  - Servidores de reserva por red (`addserver`, `delserver`, `servers`, `jump servidor`). Si un servidor falla, xBNC prueba el siguiente al momento.
  - Certificados de servidor de confianza por su huella SHA-256 (`addfingerprint`), para redes con un certificado autofirmado.
  - Dirección local (vhost) por red y por usuario (`setbindhost`, `setuserbindhost`), solo administradores.
  - Canales desactivados que siguen guardados (`disablechan`, `enablechan`) y orden de entrada (`movechan`, `swapchans`). También `nicks`, `topics`, y `attach` y `detach` con comodines.
  - Historial: `playbuffer`, `clearbuffer` con comodines, `clearallbuffers`, `clearallchannelbuffers`, `clearallquerybuffers` y `setbuffer`.
  - `addnetwork` y `delnetwork` (con confirmación), `availmods`, `reloadmod` y `uptime`.
  - Administración: `users`, `allnetworks`, `clients <usuario>`, `traffic`, `broadcast`, `movenetwork`, `rehash`, `listports`, el mensaje del día del servidor (`setmotd`, `addmotd`, `clearmotd`, `showmotd`), `restart` y `shutdown`.
- El panel marca los canales desactivados.
- Traducciones en ficheros: todos los textos de xBNC (panel, página de acceso, mensajes de IRC, ayuda de `*status`, módulos) están en `locales/<código>.json`, con el inglés como base y el español incluido.
  - Un idioma nuevo se añade copiando su fichero en `/etc/xbnc/locales/` (`locales_dir` en xbnc.toml o `XBNC_LOCALES_DIR`), sin recompilar, y aparece en todos los selectores de idioma y en `/msg *status language`. Se lee al arrancar y al recargar (`systemctl reload` o `/msg *status rehash`).
  - Lo que le falte a una traducción se muestra en inglés. Se ignoran los textos con saltos de línea, caracteres de control o marcadores distintos de los del inglés.
  - `xbncd locales check` comprueba un fichero (lo que falta, lo que sobra, marcadores y etiquetas de negrita) y `xbncd locales list` muestra los idiomas instalados con su parte traducida.
  - `TRANSLATING.md`: guía para traductores.
  - API pública `GET /api/v1/i18n` y `GET /api/v1/i18n/{code}`.
- Wiki en inglés y español en https://xbnc.org/wiki/, una web propia que cualquiera puede leer sin instalar xBNC: una página por tema (primeros pasos, uso de xBNC, funciones, administración, desarrollo, referencia, ayuda y proyecto), búsqueda en toda ella, y las páginas del proyecto (novedades, hoja de ruta, cómo contribuir, política de seguridad y licencia).
- Los 18 plugins oficiales vienen instalados: xBNC instala los que falten en el primer arranque y después de cada actualización, y actualiza los antiguos. Quedan desactivados en todas las redes hasta que cada usuario los activa. Uno desinstalado a propósito no vuelve hasta la siguiente versión.

### Corregido

- El tiempo, los títulos de enlaces y las noticias no respondían nunca: los plugins oficiales que salen a internet se instalaban sin sus sitios aprobados, así que se rechazaba cada petición. Ahora vienen aprobados (las instalaciones anteriores se reparan una vez); un sitio bloqueado por un administrador sigue bloqueado.
- Noticias: con más de una fuente solo se consultaba la primera, y cada minuto en lugar de cada intervalo. Una fuente que no se puede leer se le dice al usuario una vez y la muestra `/msg *news list`.
- Títulos de feeds y de páginas: se decodifican las entidades numéricas (`&#8217;`, `&#x2014;`) y el CDATA después de un salto de línea.
- Las peticiones web de los plugins siguen hasta 3 redirecciones, solo por https y solo a sitios aprobados (feeds que cambian de dirección, artículos de Wikipedia que son redirecciones).
- Los plugins oficiales se sustituyen por los que trae xBNC también cuando el número de versión es el mismo pero el fichero trae un arreglo.
- EvilNet se identifica con NickServ: «Automático» y el asistente de redes lo usan, y las redes añadidas antes con el login de AuthServ se envían a NickServ.
- Idioma: cambiarlo en *Ajustes* no se aplicaba a quien había elegido un idioma en la página de acceso (el panel seguía en ese). Ahora se guarda y se aplica al pulsarlo, también a quien lo cambia.
- Los plugins seen, remind, tell, poll, chanstats y antiflood activados en la 0.8.0 guardaban «es» como idioma y seguían en español después de cambiar a inglés. Al actualizar se borra ese valor antiguo y responden en el idioma de cada usuario.
- Prueba automática del cambio de idioma en el panel (`testenv/ui/language_check.py`).
- Host oculto: con `/mode $nick +x` en los comandos al conectar, se entraba en los canales antes de que la red ocultara el host, así que la IP real se veía en todos los canales. Ahora no sale ningún JOIN (los canales guardados, los de los comandos al conectar, los de un cliente o las reentradas) hasta que la red confirma el host oculto (396). El módulo Login en servicios con «Ocultar mi IP» se comporta igual: si después de la espera el host sigue sin ocultarse (un login de X fallido, por ejemplo), los canales siguen esperando y `*status` explica por qué, en lugar de entrar con la IP real. El nuevo `/msg *status joinnow` entra igualmente.
- Los errores de login en IRC dicen qué está mal y cómo arreglarlo, en el idioma del servidor: usuario o contraseña incorrectos con el formato correcto, las redes de la cuenta cuando la red no existe, y cuánto tiempo queda bloqueada una dirección y dónde desbloquearla. Copiar la contraseña de ejemplo («YOUR_PASSWORD») se explica y ya no cuenta como intento fallido, así que un cliente que reintenta con ella ya no bloquea la dirección en IRC ni en el panel.
- El mensaje de dirección bloqueada del panel dice la IP, los minutos que faltan, que un cliente que reintenta con una contraseña equivocada también cuenta, y dónde levanta el bloqueo un administrador.
- «Conecta tu cliente IRC» muestra los datos reales: el dominio o la IP indicados en la instalación (no la dirección con la que se haya abierto el panel), el puerto IRC, si el certificado es válido para ese nombre (y si no, cómo aceptarlo), y la dirección de la red local con el puerto sin SSL para conectar desde casa. El nombre del dispositivo y la contraseña se escriben en el panel para formar la contraseña de servidor completa (la contraseña no se envía ni se guarda). Antes siempre decía «laptop» y «YOUR_PASSWORD». La lista de pasos de cada cliente ya no junta pasos.
- `xbncd info` muestra la dirección que usan los clientes (el dominio o la IP configurados), la dirección de la red local, y la contraseña de servidor de cada red de cada usuario. El resumen del instalador explica dónde conectar desde internet y desde la red local.
- El certificado autofirmado de una instalación con dominio cubre también las direcciones locales de la máquina.
- La dirección del panel que muestran `xbncd info`, `xbncd setup-code`, el log y el fichero del código de configuración es la indicada en la instalación (`--ip` o el dominio), no la dirección local que indica la máquina.
- El resumen del instalador ya no dice que xBNC está en marcha cuando no se arrancó el servicio (`--no-start`, o sin systemd), y dice cómo arrancarlo.
- `xbncd info`, `setup-code`, `audit`, `check-config`, `locales` y `release` terminan sin ruido cuando su salida va a `head` u otro lector que cierra antes, en lugar de mostrar un error «Broken pipe» con traza.
- El instalador instala sus paquetes también en Arch (pacman) y openSUSE (zypper).
- Chat web en el móvil: un nombre de canal largo se corta con «…» en lugar de dibujarse debajo de tu nick, y la lista de redes ocupa menos.
- Chat web: en el móvil y en pantallas estrechas la lista de personas del canal empieza cerrada, para que los mensajes tengan todo el ancho; se abre con su botón. En un ordenador empieza abierta, como antes.

### Seguridad

- Login en servicios: en las redes sin NickServ (IRCnet, EFnet) la opción «Automático» nunca envía la contraseña, porque cualquiera podría usar ese nick; xBNC lo dice en *status y en el log.
- Cifrado FiSH: el intercambio de claves DH1080 acepta 5 peticiones por minuto y 100 claves por sesión, y el texto descifrado no puede contener saltos de línea ni caracteres nulos.

### Cambiado

- Cambiar el nombre de una red ya no corta la conexión.
- Las posiciones del historial se guardan cada 5 segundos en lugar de cada 30: se repite mucho menos después de una parada brusca.
- La búsqueda también encuentra trozos del medio de una palabra cuando no hay coincidencias de palabra completa.
- `xbncd import --utc-offset` e `import-logs` aceptan una zona horaria (`Europe/Madrid`) y aplican el horario de verano de cada día.
- DCC: si la conexión con la red usa IPv6, las ofertas anuncian la IPv4 pública del servidor, que entiende cualquier cliente.
- Las líneas que no son UTF-8 se decodifican como Windows-1252 en lugar de mostrar «�».
- El panel y los mensajes del servidor se escriben en inglés, con el español como traducción.
- Los canales guardados se listan en su orden de entrada en lugar de por orden alfabético.
- El flujo de publicación (`.github/workflows/release.yml`), que genera paquetes .deb y .rpm e imágenes de contenedor, solo se ejecuta cuando se lanza a mano: durante la beta xBNC se publica como un .zip con el código.
- La página de acceso usa los colores de xBNC (amarillo y negro) en los temas claro y oscuro, con el logo de xBNC y una ventana IRC del canal `#xbnc`.
- Todos los canales de ejemplo del panel, la ayuda, los plugins y la documentación son `#xbnc` (y `#xbnc-dev` cuando hace falta un segundo).
- La wiki ya no está dentro de la app: es un proyecto aparte (`xbnc-wiki`).
