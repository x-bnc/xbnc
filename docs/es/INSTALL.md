# Instalación y primeros pasos

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Instalación](#install)
- [Primeros pasos](#first-steps)

<a id="install"></a>

## Instalación

### Requisitos

- Cualquier Linux con systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE y otros. No hace falta instalar nada antes.
- En PC y servidores de 64 bits (x86\_64, lo normal en un VPS) el programa viene ya compilado dentro del .zip, en `bin/`, y funciona en cualquier distribución de 2019 en adelante (glibc 2.28 o posterior).
- En otras máquinas (ARM, como una Raspberry Pi o un VPS ARM) o en sistemas más antiguos, el instalador lo compila solo: instala las herramientas de compilación con el gestor de paquetes del sistema y Rust con rustup, el instalador oficial (ver *Compilar desde el código fuente*, más abajo). Mientras compila necesita 2 GB de memoria (contando la swap) y 3 GB de disco.
- 1 CPU y 512 MB de RAM para uso personal. Medido: 2 000 usuarios con sus redes y clientes en unos 150 MB (ver [Rendimiento](PERFORMANCE.md#performance)).
- Puertos: 6697 (IRC con TLS), 6667 (IRC sin TLS, solo red local), 8443 (panel). Con Let's Encrypt, también el 80; con Nginx, el 443.

El panel web viene ya compilado dentro del código: no hace falta Node.js ni npm.

### Instalar

```sh
# 1. Descomprimir el .zip (como usuario normal)
unzip xbnc-1.0.0.zip && cd xbnc

# 2. Instalarlo
sudo ./scripts/install.sh
```

Eso es todo. El instalador usa el programa ya compilado cuando hay uno para tu máquina y, si no, lo compila antes (la primera vez tarda unos minutos).

Ejecutado en una terminal y sin opciones, el instalador pregunta primero su idioma, que también será el idioma predeterminado de xBNC: inglés (por defecto), español o cualquier otro fichero de idioma de `locales/`. Después te pregunta si quieres usar un dominio, el correo para Let's Encrypt y si quieres el panel en `https://tudominio` con Nginx. Al terminar muestra la dirección del panel y el código de un solo uso para crear tu cuenta:

```
  Panel:   https://bnc.ejemplo.org:8443
  IRC:     bnc.ejemplo.org, puerto 6697 (SSL)
  Código para crear tu cuenta:  K7QM-2XPA-9DWE
```

Abre el panel, escribe el código y crea el primer administrador (será superadministrador). Para volver a ver la dirección y los puertos: `sudo xbncd info`; para volver a ver el código: `sudo xbncd setup-code`.

### Qué hace el instalador

1. Busca el programa: el ya compilado para esta máquina en `bin/`, comprobado con `bin/SHA256SUMS`, o lo compila (ver más abajo).
2. Comprueba el sistema y, si usas dominio, que el DNS apunta a esta máquina y qué puertos tienes que abrir en el router.
3. Crea el usuario del sistema `xbnc`, sin privilegios, y las carpetas con sus permisos.
4. Instala el binario en `/usr/local/bin/xbncd` y el servicio de systemd con medidas de aislamiento (`ProtectSystem=strict`, `NoNewPrivileges`, sin capacidades, filtro de llamadas al sistema).
5. Crea el certificado: propio para la IP (o el dominio), o de Let's Encrypt con renovación automática. La renovación recarga xBNC (y Nginx, si lo usa).
6. Escribe `/etc/xbnc/xbnc.toml` con la versión en la cabecera, crea la base de datos y la clave secreta.
7. Con `--nginx`, instala y configura Nginx: redirección del puerto 80, el panel en el 443, WebSockets y subida de plugins y copias de seguridad.
8. Pregunta los puertos: uno para el panel e IRC (los navegadores ven el panel y los clientes IRC el bouncer, con el mismo certificado) o uno para cada cosa. Rechaza los puertos que los navegadores no abren (el 6667 y el 6697, los habituales de IRC, están entre ellos) y los que ya usa otro programa.
9. Abre los puertos en ufw o firewalld (también el rango de DCC, 50000-50100) y arranca el servicio.

### Opciones del instalador

| Opción | Qué hace |
| --- | --- |
| `--domain NOMBRE` | Usar un dominio (bnc.ejemplo.org) en lugar de la IP. |
| `--letsencrypt CORREO` | Con --domain: certificado gratuito de Let's Encrypt con renovación automática. Necesita el puerto 80 accesible desde internet, o --dns. |
| `--dns PROVEEDOR` | Con --letsencrypt: comprobar el dominio por DNS en lugar de por el puerto 80 (cloudflare, digitalocean, ovh, linode o rfc2136). Las renovaciones funcionan igual. |
| `--dns-credentials FICHERO` | Con --dns: el fichero de credenciales del proveedor para su plugin certbot-dns (se copia con permisos 600). |
| `--nginx` | Con --domain y --letsencrypt: instala y configura Nginx para servir el panel en https://DOMINIO (puerto 443). |
| `--ip DIRECCIÓN` | IP que usar si la detectada no es la correcta (varias interfaces o VPN). |
| `--language CÓDIGO` | Idioma del instalador y predeterminado de xBNC: en, es o cualquier fichero de idioma de locales/. Sin esta opción, el instalador lo pregunta en un menú (inglés por defecto). |
| `--behind-proxy` | Panel en 127.0.0.1:8080, para publicarlo con tu propio proxy. |
| `--web-port N` | Puerto del panel (8443 por defecto). |
| `--irc-port N` | Puerto IRC con SSL (6697 por defecto). |
| `--single-port N` | Un solo puerto para el panel e IRC con SSL: https://DIRECCIÓN:N en el navegador y el mismo puerto en el cliente IRC. Los navegadores rechazan algunos puertos (6667, 6697…); el 8443 funciona. |
| `--dcc-ports A-B\|off` | Puertos para los envíos por DCC, escritos en \[dcc\] y abiertos en el cortafuegos (50000-50100 por defecto); off los deja cerrados. |
| `--plain-port N\|off` | Puerto IRC sin SSL, solo red local (6667 por defecto); off lo quita. |
| `--no-firewall` | No abrir puertos en ufw o firewalld. |
| `--reconfigure` | Rehacer la configuración y el certificado de una instalación existente (guarda una copia de la anterior). No toca usuarios, redes ni historial. |
| `--binary RUTA` | Binario que instalar. Por defecto, el ya compilado de bin/ para esta máquina, o se compila si no hay ninguno. |
| `--build` | Compilar desde el código fuente aunque haya un binario ya compilado. |
| `--no-start` | Instalar sin arrancar el servicio. |
| `--non-interactive` | No preguntar nada (por defecto pregunta si se ejecuta en una terminal). |
| `--hostname NOMBRE` | Otro nombre de --domain (sintaxis antigua); con una IP equivale a --ip. |

Para cambiar los puertos de una instalación existente, añade `--reconfigure`: sin él, el instalador conserva la configuración y rechaza las opciones de puertos que no se aplicarían.

### Compilar desde el código fuente

El instalador lo hace solo cuando no hay un programa ya compilado para tu máquina, o con `--build`. Ejecuta `scripts/prereqs.sh`, que:

1. Instala el compilador de C, `make`, `pkg-config` y `curl` con el gestor de paquetes del sistema: apt, dnf, yum, pacman, zypper o apk.
2. Instala Rust con rustup, el instalador oficial, para el usuario que descomprimió el .zip, en su carpeta personal (`~/.cargo`). El Rust que traen muchas distribuciones es demasiado antiguo, así que solo se usa si es al menos la versión 1.89; si no, se instala rustup a su lado. Si Rust ya estaba instalado con rustup pero es antiguo, se actualiza.

Después compila xBNC como ese usuario (nunca como root) con `cargo build --release --locked`, que usa exactamente las versiones de las bibliotecas con las que se ha probado. Avisa si la máquina tiene menos de 2 GB de memoria y explica cómo añadir swap.

Para ver qué falta sin instalar nada: `./scripts/prereqs.sh --check`. Para prepararlo todo y compilar a mano:

```sh
sudo ./scripts/prereqs.sh
cargo build --release          # como usuario normal; ~/.cargo/bin tiene que estar en el PATH
sudo ./scripts/install.sh       # usa target/release/xbncd
```

### Ejemplos

```sh
# Por IP, con certificado propio
sudo ./scripts/install.sh

# Con dominio y Let's Encrypt, panel en https://bnc.ejemplo.org:8443
sudo ./scripts/install.sh --domain bnc.ejemplo.org --letsencrypt tu@correo.org

# Igual, pero con el panel en https://bnc.ejemplo.org detrás de Nginx
sudo ./scripts/install.sh --domain bnc.ejemplo.org --letsencrypt tu@correo.org --nginx

# Pasar una instalación existente a dominio
sudo ./scripts/install.sh --reconfigure --domain bnc.ejemplo.org --letsencrypt tu@correo.org

# Cambiar la IP detectada
sudo ./scripts/install.sh --reconfigure --ip 192.168.1.50

# Un solo puerto para el panel e IRC: https://bnc.ejemplo.org:7000 y el puerto 7000 con SSL
sudo ./scripts/install.sh --domain bnc.ejemplo.org --letsencrypt tu@correo.org --single-port 7000

# Let's Encrypt por DNS (puerto 80 cerrado), con Cloudflare
sudo ./scripts/install.sh --domain bnc.ejemplo.org --letsencrypt tu@correo.org --dns cloudflare --dns-credentials /root/cloudflare.ini
```

### Dónde queda cada cosa

| Ruta | Contenido |
| --- | --- |
| `/usr/local/bin/xbncd` | El programa. |
| `/etc/xbnc/xbnc.toml` | Configuración del proceso (root:xbnc, 0640). |
| `/etc/xbnc/tls/` | Certificado TLS: fullchain.pem y privkey.pem (root:xbnc, 0750). |
| `/etc/xbnc/locales/` | Idiomas adicionales y correcciones de traducción (ver [Traducir xBNC](TRANSLATIONS.md#translations)). |
| `/var/lib/xbnc/` | Base de datos y clave secreta (xbnc:xbnc, 0700). **Haz copia de secret.key**: sin ella no se pueden descifrar las contraseñas guardadas. |
| `/var/lib/xbnc/logs/` | Registros en ficheros del módulo log (carpetas 0700, ficheros 0600). |
| `/var/lib/xbnc/plugins/` | Plugins instalados (0600). En `dev/`, los del modo desarrollo. |
| `/var/lib/xbnc/backups/` | Copias de seguridad hechas desde el panel. |

El servicio corre como el usuario sin privilegios `xbnc`. `sudo systemctl status xbnc`, `sudo systemctl restart xbnc`; `sudo systemctl reload xbnc` hace que relea redes, canales y usuarios cambiados con la terminal, y los ficheros de idioma. El registro: `sudo journalctl -u xbnc -f`.

### Usarlo desde fuera de casa

- Abre en el router el puerto **6697** hacia la máquina de xBNC y conéctate a tu IP pública o a un nombre de DNS dinámico (DuckDNS y similares). El 6667 no hace falta abrirlo: desde fuera xBNC lo rechaza.
- Si no quieres abrir el panel (8443) a internet, administra con un túnel SSH: `ssh -L 8443:127.0.0.1:8443 usuario@tu-ip-publica` y abre `https://localhost:8443`.
- Con un dominio que apunte a tu casa, reinstala con `--reconfigure --domain … --letsencrypt …` para quitar los avisos de certificado.

### Panel detrás de tu propio Nginx

Si ya tienes Nginx y prefieres configurarlo tú, instala con `--domain bnc.ejemplo.org --behind-proxy` y usa el ejemplo incluido:

```sh
sudo apt install -y nginx
echo 'map $http_upgrade $connection_upgrade { default upgrade; "" close; }' | sudo tee /etc/nginx/conf.d/upgrade.conf
sudo cp packaging/nginx/xbnc.conf /etc/nginx/sites-available/xbnc   # cambia server_name y certificados
sudo ln -s ../sites-available/xbnc /etc/nginx/sites-enabled/xbnc
sudo nginx -t && sudo systemctl reload nginx
```

Contenido de `packaging/nginx/xbnc.conf`:

```nginx
# Nginx reverse proxy for the xBNC web panel.
# Copy to /etc/nginx/sites-available/xbnc, adjust server_name and the
# certificate paths, then: ln -s ../sites-available/xbnc /etc/nginx/sites-enabled/
# In xbnc.toml: [web] address = "127.0.0.1:8080", public_url = "https://bnc.example.org",
# secure_cookies = true, trust_proxy = true.

server {
    listen 80;
    listen [::]:80;
    server_name bnc.example.org;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name bnc.example.org;

    ssl_certificate     /etc/letsencrypt/live/bnc.example.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/bnc.example.org/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;

    # Plugins are up to 4 MB.
    client_max_body_size 8m;

    # Restoring a backup uploads the whole archive.
    location = /api/v1/backups/restore {
        client_max_body_size 4g;
        proxy_request_buffering off;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 3600s;
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        # WebSockets: live dashboard, web chat, plugin log
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_read_timeout 3600s;
    }
}

# Needed once per Nginx configuration (e.g. in /etc/nginx/conf.d/upgrade.conf):
# map $http_upgrade $connection_upgrade { default upgrade; '' close; }
```

En `xbnc.toml`, con el panel detrás de Nginx: `address = "127.0.0.1:8080"`, `public_url = "https://bnc.ejemplo.org"`, `secure_cookies = true` y `trust_proxy = true`.

### Certificado de otra autoridad

Copia tu certificado y tu clave a `/etc/xbnc/tls/fullchain.pem` y `/etc/xbnc/tls/privkey.pem` (propietario root:xbnc, la clave con permisos 0640) y reinicia con `sudo systemctl restart xbnc`. El certificado se lee al arrancar; `reload` no basta.

### Docker

```sh
mkdir -p tls
cp /ruta/fullchain.pem /ruta/privkey.pem tls/
# Edita XBNC_HOSTNAME y XBNC_PUBLIC_URL en docker-compose.yml
docker compose up -d --build
docker compose logs xbnc | grep "setup code"
```

Abre `https://bnc.ejemplo.org:8443/`, escribe el código y crea tu cuenta. La terminal: `docker compose exec xbnc xbncd user add admin --admin`. Los datos viven en el volumen `xbnc-data`. El contenedor se ejecuta como el usuario sin privilegios `xbnc` (uid 10001), con sistema de ficheros de solo lectura, sin capacidades y con `no-new-privileges`.

| Variable | Uso |
| --- | --- |
| `XBNC_HOSTNAME` | Nombre del servidor (solo en el primer arranque). |
| `XBNC_TLS_CERT`, `XBNC_TLS_KEY` | Rutas del certificado y la clave dentro del contenedor; activan TLS en el 6697 (IRC) y el 8443 (panel). |
| `XBNC_INSECURE=1` | Texto plano en el 6667 (IRC) y el 8080 (panel), solo para pruebas. |
| `XBNC_PUBLIC_URL` | URL pública del panel. |
| `XBNC_WEB=0` | Desactiva el panel. |
| `XBNC_LOCALES_DIR` | Carpeta de idiomas adicionales (ver [Traducir xBNC](TRANSLATIONS.md#translations)). |
| `RUST_LOG` | Nivel de registro, por ejemplo info o debug. |

### Actualizar

```sh
unzip xbnc-nueva.zip && cd xbnc     # o git pull
sudo ./scripts/update.sh
```

`update.sh` toma el programa nuevo igual que el instalador (el ya compilado de `bin/`, o lo compila; `--build` obliga a compilar), lo comprueba junto con la configuración, hace una copia de seguridad, para el servicio, cambia el binario y lo arranca. Si no arranca, vuelve al binario anterior **y** a la base de datos de antes de actualizar (un binario nuevo puede haber actualizado el esquema, y uno antiguo se niega a abrir un esquema más nuevo). Nunca se actualiza nada solo. Con `--binary RUTA` instala otro binario.

Si además quieres rehacer la configuración (por ejemplo, para pasar a dominio), ejecuta después `sudo ./scripts/install.sh --reconfigure`.

### Desinstalar

```sh
sudo ./scripts/uninstall.sh            # quita el servicio y el binario; conserva /etc/xbnc y /var/lib/xbnc
sudo ./scripts/uninstall.sh --purge    # borra también la configuración, los datos y el usuario xbnc
```

> El puerto 6667 sin SSL solo acepta conexiones de tu red local (direcciones privadas, loopback, enlace local, CGNAT/Tailscale e IPv6 ULA). Desde internet xBNC lo rechaza con un mensaje que indica el 6697.

<a id="first-steps"></a>

## Primeros pasos

1. **Crea tu cuenta** en el panel con el código de un solo uso. La primera cuenta es superadministrador.
2. **Añade una red** en *Redes → Añadir red*. El asistente tiene cuatro pasos: elegir la red (las conocidas vienen preparadas), tu nick y nombre, la identificación (SASL, X, Q…) y los canales.
3. **Conecta tu cliente**. La página de la red tiene la tarjeta «Conecta tu cliente IRC» con tus datos exactos y los pasos para HexChat, mIRC, WeeChat e irssi, con botones para copiar.
4. **Activa módulos y plugins** en *Módulos*. Elige la red arriba; con muchas redes, «Copiar a otras redes» repite la configuración.
5. **Elige tu zona horaria e idioma** en *Tu cuenta*: la zona se usa en los registros y en las tareas programadas.
6. **Activa la verificación en dos pasos** y, si quieres, una passkey, en *Tu cuenta*.

### Redes preparadas en el asistente

| Red | Servidor | Puerto | TLS | Identificación | Descripción |
| --- | --- | --- | --- | --- | --- |
| Undernet | `irc.undernet.org` | 6667 | no | X con Login on Connect | Red clásica con canales de todo tipo |
| Libera.Chat | `irc.libera.chat` | 6697 | sí | SASL | Comunidades de software libre y proyectos de código abierto |
| freenode | `irc.freenode.net` | 6697 | sí | SASL | Red histórica de proyectos y comunidades |
| Rizon | `irc.rizon.net` | 6697 | sí | SASL o NickServ | Anime, juegos y comunidades variadas |
| EvilNet | `irc.evilnet.org` | 6697 | sí | NickServ | Red con servicios NickServ |
| IRCnet | `open.ircnet.net` | 6697 | sí | sin NickServ | Red histórica, muy popular en Europa |
| OFTC | `irc.oftc.net` | 6697 | sí | NickServ | Debian, Tor, el núcleo de Linux y otros proyectos |
| EFnet | `irc.efnet.org` | 6697 | sí | sin servicios | Una de las redes más antiguas |
| QuakeNet | `irc.quakenet.org` | 6667 | no | Q | Juegos y comunidades europeas |
| DALnet | `irc.dal.net` | 6697 | sí | SASL | Comunidades sociales y de ayuda |
| hackint | `irc.hackint.org` | 6697 | sí | SASL | Hackerspaces y comunidades técnicas |

> Undernet y QuakeNet no ofrecen TLS: la conexión con la red va por el 6667 sin cifrar. La conexión entre tu cliente y xBNC sí va cifrada.

### Lo mismo desde la terminal

```sh
sudo -u xbnc xbncd user add admin --admin
sudo -u xbnc xbncd network add admin libera --host irc.libera.chat --port 6697 --sasl-user micuenta
sudo -u xbnc xbncd channel add admin libera '#xbnc'
sudo systemctl reload xbnc
```

La contraseña SASL se pide al momento (o se lee de `XBNC_SASL_PASSWORD`) y se guarda cifrada.
