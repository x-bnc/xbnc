# Instalación y primeros pasos

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Instalación](#install)
- [Guía paso a paso para principiantes](#beginner-guide)
- [La carpeta scripts](#scripts)
- [Primeros pasos](#first-steps)

<a id="install"></a>

## Instalación

### Requisitos

- Cualquier Linux con systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE y otros.
- **git**, para descargar el código de GitHub (`sudo apt install git`, `sudo dnf install git`…). Del resto se encarga el instalador.
- Para compilar xBNC: unos **2 GB de memoria** (la swap cuenta; el instalador avisa si hay menos y explica cómo añadirla) y **3 GB de disco libre**. Compilar solo hace falta al instalar o actualizar; para funcionar, xBNC necesita mucho menos (siguiente punto).
- Para funcionar: 1 CPU y 512 MB de RAM para uso personal. Medido: 2 000 usuarios con sus redes y clientes en unos 150 MB (ver [Rendimiento](PERFORMANCE.md#performance)).
- Puertos: con la respuesta recomendada del instalador, el **8443** para el panel e IRC con SSL a la vez; con un puerto para cada cosa, el 8443 para el panel y el 6697 para IRC con SSL. Además, el 6667 para IRC sin SSL (solo red local) y el 50000-50100 para DCC. Con Let's Encrypt, también el 80; con Nginx, el 80 y el 443.

El panel web viene ya compilado dentro del código: no hace falta Node.js ni npm.

### Instalar desde GitHub

```sh
# 1. Descargar el código (como usuario normal, no como root)
git clone https://github.com/x-bnc/xbnc.git

# 2. Entrar en su carpeta
cd xbnc

# 3. Instalarlo
sudo ./scripts/install.sh
```

Eso es todo. El código de GitHub no trae el programa compilado, así que la primera vez el instalador lo compila en tu servidor: instala las herramientas de compilación y Rust, compila xBNC (varios minutos; más en un VPS pequeño) y lo instala. Si nunca has montado un servidor, sigue la [guía paso a paso](#beginner-guide).

### Con el .zip de una versión

Cada versión estable publicada en [GitHub Releases](https://github.com/x-bnc/xbnc/releases) tiene también un `.zip` con el programa ya compilado para PC y servidores de 64 bits (x86\_64, lo normal en un VPS), en `bin/` y comprobado con `bin/SHA256SUMS`. Funciona en cualquier distribución de 2019 en adelante (glibc 2.28 o posterior) y no hay que compilar nada:

```sh
unzip xbnc-1.0.0.zip && cd xbnc
sudo ./scripts/install.sh
```

> El .zip solo existe cuando se ha publicado una versión estable en Releases. `git clone` funciona siempre: solo tarda más porque compila el programa.

### Las preguntas del instalador

Ejecutado en una terminal y sin opciones, el instalador pregunta:

1. **El idioma** del instalador, que también será el idioma predeterminado de xBNC: inglés (por defecto), español o cualquier otro fichero de idioma de `locales/`.
2. **Si quieres usar un dominio.** Por defecto no: xBNC funciona con la IP de la máquina, que detecta solo. Con dominio pregunta además si pedir un certificado gratuito de Let's Encrypt, el correo para sus avisos, si se puede llegar al puerto 80 desde internet (si no, el proveedor de DNS y su fichero de credenciales) y si servir el panel en `https://tudominio` con Nginx.
3. **Los puertos.** Un puerto para el panel e IRC, el 8443 por defecto (los navegadores ven el panel y los clientes IRC el bouncer, con el mismo certificado), o uno para cada cosa: el 8443 para el panel y el 6697 para IRC. Con Nginx solo pregunta el puerto IRC (6697). Rechaza los puertos que ya usa otro programa y, para el panel, los que los navegadores no abren (el 6667 y el 6697, los habituales de IRC, están entre ellos).

Con `--non-interactive`, o sin terminal, no pregunta nada: usa las opciones que le des y, sin opciones de puertos, pone el panel en el 8443 e IRC en el 6697. En todos los casos añade además el puerto 6667 sin SSL para la red local; `--plain-port` lo cambia o lo quita.

Al terminar muestra un resumen como este (aquí, instalado por IP con las respuestas predeterminadas):

```
  ┌──────────────────────────────────────────────────────────────
  │  xBNC está instalado y funcionando
  │
  │  1. Abre el panel:  https://203.0.113.10:8443  (el mismo puerto que IRC)
  │     El navegador avisará del certificado (es propio): pulsa
  │     «Avanzado» y «Continuar» / «Aceptar el riesgo».
  │  2. Escribe el código de instalación:  K7QM-2XPA-9DWE
  │     y crea tu usuario administrador.
  │  3. Añade tus redes en «Redes → Añadir red».
  │
  │  Cliente IRC (HexChat, mIRC, WeeChat, irssi…):
  │    Servidor 203.0.113.10, puerto 8443 con SSL
  │    Desde tu red local: servidor 203.0.113.10, puerto 6667 sin SSL
  │    Contraseña del servidor: tu usuario/tu red@dispositivo:tu contraseña
  │    (el usuario y la contraseña del paso 2 y la red del paso 3).
  │    El panel te la da lista para copiar en «Redes → tu red», y
  │    «sudo xbncd info» la muestra con tus usuarios y redes reales.
  └──────────────────────────────────────────────────────────────

  Más adelante:
    sudo xbncd info          dirección del panel y puertos
    sudo xbncd setup-code    código de instalación
```

Abre el panel, escribe el código y crea el primer administrador (será superadministrador). La línea «Desde tu red local» es para una máquina en casa; en un VPS, no le hagas caso. Para volver a ver la dirección y los puertos: `sudo xbncd info`; para volver a ver el código: `sudo xbncd setup-code`.

### Qué hace el instalador

1. Hace las preguntas de arriba y, si usas dominio, comprueba que el DNS apunta a esta máquina y te dice qué puertos tienes que abrir en el router.
2. Busca el programa: el ya compilado para esta máquina en `bin/` (solo en el .zip de una versión), comprobado con `bin/SHA256SUMS`, o lo compila (ver más abajo).
3. Instala lo que necesita con el gestor de paquetes del sistema: `ca-certificates` y `openssl`, además de `certbot` con Let's Encrypt y `nginx` con `--nginx`.
4. Crea el usuario del sistema `xbnc`, sin privilegios, y las carpetas con sus permisos.
5. Instala el binario en `/usr/local/bin/xbncd` y la documentación en `/usr/local/share/doc/xbnc`.
6. Crea el certificado: propio para 10 años, para la IP (o el dominio) y las demás direcciones de la máquina, o de Let's Encrypt con renovación automática. Cada renovación copia el certificado nuevo, reinicia xBNC y recarga Nginx si lo usa.
7. Escribe `/etc/xbnc/xbnc.toml` con la versión en la cabecera, crea la base de datos y la clave secreta.
8. Con `--nginx`, configura Nginx: redirección del puerto 80, el panel en el 443, WebSockets y subida de plugins y copias de seguridad.
9. Abre los puertos en ufw o firewalld: el puerto IRC, el del panel cuando es otro distinto, el rango de DCC (50000-50100) y el 80 y el 443 cuando hacen falta. Con ufw, el 6667 solo se abre a las redes privadas.
10. Instala el servicio de systemd con medidas de aislamiento (`ProtectSystem=strict`, `NoNewPrivileges`, sin capacidades, filtro de llamadas al sistema) y lo arranca. Arranca solo en cada inicio del sistema y se reinicia si falla.

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

Para cambiar los puertos de una instalación existente, añade `--reconfigure`: sin él, el instalador conserva la configuración y rechaza las opciones de puertos que no se aplicarían. `--reconfigure` vuelve a escribir `xbnc.toml` con las respuestas y opciones de esa ejecución, no con el fichero actual: en una terminal vuelve a hacer las preguntas, y sin terminal (o con `--non-interactive`) los puertos que no indiques toman sus valores por defecto (8443 para el panel y 6697 para IRC). Los cambios que hiciste a mano en `xbnc.toml` quedan en la copia que guarda (`xbnc.toml.bak-FECHA`).

### Compilar desde el código fuente

El instalador lo hace solo cuando no hay un programa ya compilado para tu máquina (siempre con `git clone`), o con `--build`. Ejecuta `scripts/prereqs.sh`, que:

1. Instala el compilador de C, `make`, `pkg-config` y `curl` con el gestor de paquetes del sistema: apt, dnf, yum, pacman, zypper o apk.
2. Instala Rust con rustup, el instalador oficial, para el dueño de la carpeta xbnc (el usuario que la clonó o la descomprimió), en su carpeta personal (`~/.cargo`). El Rust que traen muchas distribuciones es demasiado antiguo, así que solo se usa si es al menos la versión 1.89; si no, se instala rustup a su lado. Si Rust ya estaba instalado con rustup pero es antiguo, se actualiza.

Después compila xBNC como ese usuario (no como el que ejecutó sudo) con `cargo build --release --locked`, que usa exactamente las versiones de las bibliotecas con las que se ha probado. Avisa si la máquina tiene menos de 2 GB de memoria y explica cómo añadir swap.

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
| `/usr/local/share/doc/xbnc/` | La documentación, la lista de cambios, la licencia y los ficheros de idioma de la versión instalada. |
| `/etc/xbnc/xbnc.toml` | Configuración del proceso (root:xbnc, 0640). |
| `/etc/xbnc/tls/` | Certificado TLS: fullchain.pem y privkey.pem (root:xbnc, 0750). |
| `/etc/xbnc/locales/` | Idiomas adicionales y correcciones de traducción (ver [Traducir xBNC](TRANSLATIONS.md#translations)). |
| `/var/lib/xbnc/` | Base de datos y clave secreta (xbnc:xbnc, 0700). **Haz copia de secret.key**: sin ella no se pueden descifrar las contraseñas guardadas. |
| `/var/lib/xbnc/logs/` | Registros en ficheros del módulo log (carpetas 0700, ficheros 0600). |
| `/var/lib/xbnc/plugins/` | Plugins instalados (0600). En `dev/`, los del modo desarrollo. |
| `/var/lib/xbnc/backups/` | Copias de seguridad hechas desde el panel. |

El servicio corre como el usuario sin privilegios `xbnc`. `sudo systemctl status xbnc`, `sudo systemctl restart xbnc`; `sudo systemctl reload xbnc` hace que relea redes, canales y usuarios cambiados con la terminal, y los ficheros de idioma. El registro: `sudo journalctl -u xbnc -f`.

### Usarlo desde fuera de casa

- Abre en el router el puerto IRC con SSL hacia la máquina de xBNC (el 8443 si elegiste un solo puerto para el panel e IRC, el 6697 si elegiste uno para cada cosa; el instalador te dice cuál) y conéctate a tu IP pública o a un nombre de DNS dinámico (DuckDNS y similares). El 6667 no hace falta abrirlo: desde fuera xBNC lo rechaza.
- Con un puerto para cada cosa, si no quieres abrir el panel (8443) a internet, administra con un túnel SSH: `ssh -L 8443:127.0.0.1:8443 usuario@tu-ip-publica` y abre `https://localhost:8443`. Con un solo puerto, el panel e IRC van juntos: al abrir el puerto se abren los dos.
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
# Instalado con git clone: descargar los cambios y compilarlos
cd ~/xbnc
git pull
sudo ./scripts/update.sh --build

# Instalado con el .zip: descomprimir el nuevo en una carpeta vacía
mkdir ~/xbnc-nueva && cd ~/xbnc-nueva
unzip ~/xbnc-VERSION-NUEVA.zip && cd xbnc
sudo ./scripts/update.sh
```

`update.sh` toma el programa nuevo igual que el instalador (el ya compilado de `bin/`, o lo compila; `--build` obliga a compilar), lo comprueba junto con la configuración, hace una copia de seguridad, para el servicio, cambia el binario y lo arranca. Si no arranca, vuelve al binario anterior **y** a la base de datos de antes de actualizar (un binario nuevo puede haber actualizado el esquema, y uno antiguo se niega a abrir un esquema más nuevo). Nunca se actualiza nada solo. Con `--binary RUTA` instala otro binario.

> Después de `git pull`, usa siempre `--build`: sin él, update.sh toma el programa ya compilado en `target/` por la instalación anterior, que es la versión antigua.

Si además quieres rehacer la configuración (por ejemplo, para pasar a dominio), ejecuta después `sudo ./scripts/install.sh --reconfigure`.

### Desinstalar

```sh
sudo ./scripts/uninstall.sh            # quita el servicio y el binario; conserva /etc/xbnc y /var/lib/xbnc
sudo ./scripts/uninstall.sh --purge    # borra también la configuración, los datos y el usuario xbnc
```

> El puerto 6667 sin SSL solo acepta conexiones de tu red local (direcciones privadas, loopback, enlace local, CGNAT/Tailscale e IPv6 ULA). Desde internet xBNC lo rechaza con un mensaje que indica el puerto con SSL.

<a id="beginner-guide"></a>

## Guía paso a paso para principiantes

Esta guía es para quien nunca ha montado un servidor. Va desde cero hasta chatear a través de xBNC, paso a paso. Copia cada comando, pégalo en la terminal y pulsa Intro.

### Qué necesitas

- **Un servidor Linux encendido siempre.** xBNC es persistente porque vive allí y no en tu ordenador. Lo más fácil es alquilar un VPS (un servidor virtual pequeño, unos pocos euros al mes) con Debian 12 o Ubuntu 24.04. También vale una Raspberry Pi o un ordenador de casa que no se apague nunca.
- **Al menos 1 GB de RAM.** Compilar xBNC necesita unos 2 GB contando la swap, y el paso 4 explica cómo añadir swap si te falta.
- **La dirección IP del servidor y acceso como root** (o un usuario con sudo). Tu proveedor de VPS te da las dos cosas al crearlo.
- **Un cliente IRC** en el ordenador o el móvil (HexChat, mIRC, WeeChat, irssi, Goguma…), o solo el navegador: el panel tiene chat web.

### Paso 1. Conéctate al servidor

Abre una terminal: en Windows, *PowerShell* o *Terminal de Windows*; en macOS y Linux, *Terminal*. Conéctate por SSH con la IP de tu servidor (aquí 203.0.113.10 como ejemplo):

```sh
ssh root@203.0.113.10
```

La primera vez pregunta si confías en el servidor: escribe `yes`. Después escribe la contraseña (mientras escribes no aparece nada; es normal).

### Paso 2. Crea un usuario normal

No instales como root: el instalador compila xBNC como el dueño de su carpeta, y es más seguro hacerlo con un usuario normal. Si tu proveedor ya te dio un usuario con sudo, sáltate este paso.

```sh
# Debian y Ubuntu
adduser alicia
usermod -aG sudo alicia

# Fedora, Rocky, Alma y RHEL
adduser alicia && passwd alicia
usermod -aG wheel alicia

# Después, en cualquier sistema, entra como ese usuario
su - alicia
```

Pon tu propio nombre en lugar de `alicia`. A partir de aquí, los comandos que empiezan por `sudo` te pedirán la contraseña de ese usuario.

### Paso 3. Instala git

git es el programa que descarga el código de GitHub.

```sh
sudo apt update && sudo apt install -y git      # Debian y Ubuntu
sudo dnf install -y git                         # Fedora, Rocky, Alma y RHEL
sudo pacman -S git                              # Arch
sudo zypper install git                         # openSUSE
```

Ejecuta solo la línea de tu sistema.

### Paso 4. Comprueba la memoria y el disco

```sh
free -h      # memoria: suma el "total" de Mem y de Swap
df -h ~      # disco: "Disp" (o "Avail") debe ser al menos 3G
```

Si la memoria y la swap juntas suman menos de 2 GB, añade 2 GB de swap (un fichero en el disco que hace de memoria extra). Solo hace falta para compilar, pero no molesta dejarla:

```sh
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### Paso 5. Descarga xBNC

```sh
cd ~
git clone https://github.com/x-bnc/xbnc.git
cd xbnc
```

Ahora tienes una carpeta llamada `xbnc` en tu carpeta personal con todo el código, y estás dentro de ella. No la borres: sirve para actualizar.

### Paso 6. Ejecuta el instalador

```sh
sudo ./scripts/install.sh
```

El instalador hace unas pocas preguntas. Si no estás seguro, pulsa Intro: toma el valor que aparece entre corchetes, que es el recomendado.

| Pregunta | Qué contestar |
| --- | --- |
| Idioma | El idioma del instalador, que también será el idioma predeterminado de xBNC. Escribe el número del idioma; el 1, inglés, es el predeterminado. |
| ¿Quieres usar un dominio? | Si no tienes dominio, contesta que no (lo predeterminado): xBNC funciona con la IP. Si tienes uno (bnc.ejemplo.org) y ya apunta a este servidor, contesta que sí y escríbelo. |
| ¿Pedir un certificado gratuito de Let's Encrypt? | Solo con dominio. Sí consigue un certificado en el que confían navegadores y clientes, sin avisos. Pide un correo para sus avisos. |
| ¿Se puede llegar al puerto 80 de esta máquina desde internet? | Solo con Let's Encrypt, que comprueba el dominio por el puerto 80. Si no puedes abrirlo, contesta que no: entonces pide tu proveedor de DNS (Cloudflare, DigitalOcean, OVH, Linode o RFC 2136) y su fichero de credenciales. |
| ¿Servir el panel en https://DOMINIO con Nginx? | Solo con dominio y Let's Encrypt. Sí deja el panel en una dirección normal, sin número de puerto. |
| ¿Usar un solo puerto para el panel e IRC? | Sí (lo predeterminado): el panel y los clientes IRC comparten el puerto 8443. No: un puerto para cada uno (8443 para el panel y 6697 para IRC). Con Nginx no se pregunta: el panel va en el 443 y solo pide el puerto IRC (6697). |
| Puerto… | Pulsa Intro para aceptar el puerto propuesto. Si otro programa lo está usando, el instalador lo dice y pide otro. |

Después trabaja solo: instala las herramientas de compilación y Rust, muestra *Compilando xBNC como alicia (tarda unos minutos)* y, al terminar, crea el certificado, abre los puertos en el cortafuegos e instala y arranca el servicio. Al final muestra un resumen como este:

```
  ┌──────────────────────────────────────────────────────────────
  │  xBNC está instalado y funcionando
  │
  │  1. Abre el panel:  https://203.0.113.10:8443  (el mismo puerto que IRC)
  │     El navegador avisará del certificado (es propio): pulsa
  │     «Avanzado» y «Continuar» / «Aceptar el riesgo».
  │  2. Escribe el código de instalación:  K7QM-2XPA-9DWE
  │     y crea tu usuario administrador.
  │  3. Añade tus redes en «Redes → Añadir red».
  │
  │  Cliente IRC (HexChat, mIRC, WeeChat, irssi…):
  │    Servidor 203.0.113.10, puerto 8443 con SSL
  │    Desde tu red local: servidor 203.0.113.10, puerto 6667 sin SSL
  │    Contraseña del servidor: tu usuario/tu red@dispositivo:tu contraseña
  │    (el usuario y la contraseña del paso 2 y la red del paso 3).
  │    El panel te la da lista para copiar en «Redes → tu red», y
  │    «sudo xbncd info» la muestra con tus usuarios y redes reales.
  └──────────────────────────────────────────────────────────────

  Más adelante:
    sudo xbncd info          dirección del panel y puertos
    sudo xbncd setup-code    código de instalación
```

Apunta la dirección del panel, el código y el puerto IRC. En un VPS, no hagas caso de la línea «Desde tu red local»: es para una máquina en casa. Si los pierdes: `sudo xbncd info` muestra la dirección del panel, los puertos IRC y la contraseña del servidor de cada red, y `sudo xbncd setup-code` muestra el código.

### Paso 7. Abre el panel y crea tu cuenta

1. Abre la dirección del panel en el navegador.
2. Si instalaste con la IP, el navegador avisa de que la conexión no es privada. Es lo esperado: el certificado lo ha hecho tu propio servidor y no una autoridad. Elige *Configuración avanzada* y continúa al sitio. Con dominio y Let's Encrypt no hay aviso.
3. Escribe el código y crea tu usuario y contraseña. Esta primera cuenta es la de superadministrador.
4. Recomendado: activa la verificación en dos pasos en *Tu cuenta*.

### Paso 8. Añade tu red IRC

En *Redes → Añadir red*, elige tu red de la lista (o escribe su servidor, por ejemplo `irc.ejemplo.org`), tu nick, cómo te identificas y los canales, por ejemplo `#xbnc`. En cuanto la guardas, xBNC se conecta a la red y entra en los canales, aunque no haya ningún cliente conectado. Ver [Primeros pasos](#first-steps).

### Paso 9. Conecta tu cliente IRC

En el panel, *Redes → tu red → Conecta tu cliente IRC* muestra tus datos exactos, con los pasos para HexChat, mIRC, WeeChat e irssi. En resumen:

- **Servidor:** la IP o el dominio del resumen. **Puerto:** el de la línea «puerto … con SSL» del resumen (el 8443 si elegiste un solo puerto, el 6697 si elegiste uno para cada cosa), con SSL activado.
- **Contraseña del servidor:** `alicia/red:tu-contraseña`, con tu usuario, el nombre de la red en xBNC y la contraseña del panel.
- Si instalaste con la IP, activa «aceptar certificados no válidos» en el cliente, por lo mismo que en el paso 7.

Más detalles en [Conectar tu cliente IRC](CLIENTS.md#clients).

### Paso 10. Comprueba que es persistente

Cierra el cliente IRC, o apaga el ordenador, y vuelve al cabo de un rato. Tu nick ha seguido en los canales y, al conectarte de nuevo, recibes los mensajes que te perdiste. Eso es lo que hace un bouncer.

### Comandos útiles para después

| Para… | Ejecuta |
| --- | --- |
| Ver si xBNC está funcionando | `sudo systemctl status xbnc` |
| Ver la dirección del panel, los puertos IRC y las contraseñas del servidor | `sudo xbncd info` |
| Ver la versión instalada | `xbncd --version` |
| Ver los últimos mensajes del registro | `sudo journalctl -u xbnc -n 50` |
| Reiniciar xBNC | `sudo systemctl restart xbnc` |
| Actualizar a la última versión | `cd ~/xbnc && git pull && sudo ./scripts/update.sh --build` |
| Hacer una copia de seguridad | `cd ~/xbnc && sudo ./scripts/backup.sh` |

Si algo falla, mira en [Problemas frecuentes](TROUBLESHOOTING.md#troubleshooting). Cada script está explicado en [La carpeta scripts](#scripts).

<a id="scripts"></a>

## La carpeta scripts

La carpeta `scripts/` del código tiene todo lo necesario para instalar y cuidar xBNC. Ejecútalos desde la carpeta `xbnc`, con `sudo`.

| Script | Para qué sirve | Cómo se usa |
| --- | --- | --- |
| `install.sh` | Instala xBNC: el programa, el usuario del sistema, el servicio, el certificado, la configuración y el cortafuegos. También cambia una instalación existente con `--reconfigure`. | `sudo ./scripts/install.sh`<br>Opciones en [Instalación](#install). |
| `update.sh` | Actualiza a una versión nueva: comprueba el programa nuevo y la configuración, hace una copia de seguridad, cambia el binario y reinicia. Si xBNC no arranca, vuelve solo a la versión anterior. Solo se ejecuta cuando tú lo llamas. | `sudo ./scripts/update.sh` con el .zip.<br>`git pull` y después `sudo ./scripts/update.sh --build` con git: --build compila el código nuevo. |
| `backup.sh` | Hace una copia de seguridad coherente con xBNC en marcha: la base de datos, la clave secreta y `/etc/xbnc`, en un .tar.gz en `/var/backups/xbnc`. El fichero tiene permisos 0600 porque contiene credenciales: guárdalo en un sitio seguro. | `sudo ./scripts/backup.sh [CARPETA] [--keep N]`<br>`--keep` guarda solo las N últimas (14 por defecto). |
| `restore.sh` | Restaura una copia hecha con backup.sh. Antes de sustituir nada guarda la base de datos y la clave actuales como `*.pre-restore-FECHA`. | `sudo ./scripts/restore.sh FICHERO [--with-config] [--yes]`<br>`--with-config` restaura también `/etc/xbnc`. |
| `uninstall.sh` | Quita el servicio y el programa. Por defecto conserva la configuración y los datos, por si vuelves a instalar. | `sudo ./scripts/uninstall.sh`<br>`--purge` borra también la configuración, los datos y el usuario xbnc, después de pedirte que escribas `delete` (`--purge --yes` no pregunta). Las copias de /var/backups/xbnc se conservan. |
| `prereqs.sh` | Instala lo necesario para compilar: el compilador de C, make, pkg-config, curl y Rust. El instalador y update.sh lo ejecutan solos cuando tienen que compilar. | `./scripts/prereqs.sh --check` solo dice lo que falta.<br>`sudo ./scripts/prereqs.sh` lo instala. |
| `binary.sh` | Busca el programa que instalar (el ya compilado de `bin/` del .zip) o lo compila, y antes comprueba la memoria y el disco. Forma parte de install.sh y update.sh. | No se ejecuta a mano. |
| `i18n.sh` | Carga los textos del instalador desde los ficheros de idioma de `locales/`, para que los scripts hablen tu idioma. Forma parte de los demás scripts. | No se ejecuta a mano. |

> Las copias de seguridad contienen la clave secreta y la configuración con credenciales. No las compartas ni las dejes en una carpeta pública.

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
sudo -u xbnc xbncd network add admin ejemplo --host irc.ejemplo.org --port 6697 --sasl-user micuenta
sudo -u xbnc xbncd channel add admin ejemplo '#xbnc'
sudo systemctl reload xbnc
```

La contraseña SASL se pide al momento (o se lee de `XBNC_SASL_PASSWORD`) y se guarda cifrada.
