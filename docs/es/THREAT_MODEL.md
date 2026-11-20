<a id="threat-model"></a>

# Modelo de amenazas

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Qué protege xBNC, de quién y con qué medida en cada punto. Es la guía para la auditoría externa y para revisar cada cambio.

### Qué se protege

| Activo | Por qué importa |
| --- | --- |
| Contraseñas de las cuentas de xBNC | Dan acceso a las redes, al historial y, a los administradores, al servidor entero. |
| Credenciales de red (SASL, contraseña de servidor, claves de canal, comandos al conectar, contraseñas de servicios, claves FiSH, certificados) | Permiten suplantar al usuario en las redes. |
| Historial y registros | Privados y canales: información personal. |
| Sesiones del panel y tokens de API | Equivalen a la contraseña mientras son válidos. |
| La clave secreta (secret.key) | Descifra todas las credenciales guardadas. |
| La disponibilidad del servicio | Si cae, los usuarios pierden mensajes y presencia. |
| La reputación de la IP del servidor | Todas las conexiones salen de ella. |

### Quién puede atacar

| Actor | Qué controla | Qué intenta |
| --- | --- | --- |
| Cualquiera en internet | Conexiones a los puertos de IRC, panel y DCC | Adivinar contraseñas, agotar recursos, explotar el parser. |
| Alguien en una red IRC | Mensajes, CTCP, DCC, nombres de canal y nick | Inyectar comandos, que xBNC se conecte a redes internas, saturar el historial. |
| Un servidor IRC malicioso | Todo lo que llega por la conexión de una red | Lo anterior, más respuestas malformadas, líneas enormes y desconexiones en bucle. |
| Un usuario de xBNC | Su cuenta, sus redes, módulos y plugins | Leer datos de otros, escalar a administrador, abusar del servidor. |
| Un administrador | Usuarios y redes de otros | Escalar a superadministrador, cambiar ajustes globales. |
| El autor de un plugin | El código que instala un superadministrador | Salir del aislamiento, leer datos, actuar sin permiso. |
| El autor de un fichero de idioma | Un fichero que un administrador copia en locales\_dir | Meter comandos IRC o HTML con los textos (se rechaza: se validan y se tratan como texto plano). |
| Una web maliciosa | El navegador de un usuario con sesión | CSRF, robo de sesión, secuestro del WebSocket. |
| Quien accede a las copias de seguridad | Los .tar.gz | Credenciales y conversaciones. |

Fuera de alcance: quien ya es root en la máquina, el proveedor del servidor y los fallos del sistema operativo o de las redes IRC.

### Conexiones de clientes IRC (6697/6667)

| Amenaza | Medida |
| --- | --- |
| Fuerza bruta de contraseñas | Bloqueo por IP compartido con el panel y los tokens; Argon2id; mismo coste para usuarios inexistentes. |
| Agotar conexiones o memoria | Límite global de conexiones, 16 sin autenticar por IP, clientes por usuario, tiempo de registro y colas acotadas. |
| Líneas enormes o binarias | Límite de 8703 bytes por línea y UTF-8 con pérdida. |
| Inyección de comandos con CR/LF | Cualquier CR o LF corta la línea; los textos de módulos, plugins y ficheros de idioma se validan sin caracteres de control. |
| Texto plano en internet | Los puertos sin TLS solo aceptan la red local o exigen insecure = true. |
| Credenciales en los registros | Enmascarado de PASS, AUTHENTICATE, OPER y mensajes a servicios en todos los niveles. |

### Conexiones a las redes IRC

| Amenaza | Medida |
| --- | --- |
| Suplantación del servidor | TLS verificado con el almacén del sistema, sin opción para desactivarlo; un certificado de confianza solo acepta su huella SHA-256 exacta. |
| Respuestas malformadas | Parser propio sin unsafe, con tests y fuzzing. |
| Inundación del historial | Retención configurable, eventos opcionales, colas acotadas; el módulo log descarta líneas si el disco no da abasto; ocultar canales inundados. |
| Desconexiones en bucle | Espera progresiva con variación aleatoria y espera mínima tras avisos de throttling. |
| Expulsión por flood | Control de ritmo en la salida. |
| Robo de la contraseña de servicios | En redes sin NickServ nunca se envía en modo automático. |

### DCC

| Amenaza | Medida |
| --- | --- |
| Usar el bouncer para llegar a redes internas (SSRF) | Ofertas hacia direcciones internas o puertos por debajo de 1024 rechazadas salvo permiso del usuario. |
| Hacer que xBNC se conecte a un tercero | En tus envíos solo se conecta de vuelta a la IP de tu dispositivo. |
| Puertos abiertos indefinidamente | Un puerto por oferta, una sola conexión, tiempo de espera, cierre por inactividad y límite de tamaño. |
| Robo de la transferencia | En la recepción, el puerto solo acepta la IP del cliente. |

### Panel web y API

| Amenaza | Medida |
| --- | --- |
| CSRF | Token CSRF en cabecera, cookie SameSite=Strict y comprobación de Origin. |
| XSS | React escapa todo el contenido; CSP sin scripts externos ni en línea; los textos de IRC nunca se interpretan como HTML; las traducciones son texto plano. |
| Robo de sesión | Cookie HttpOnly y Secure; solo el SHA-256 del token en la base de datos; caducidad deslizante; cambiar la contraseña cierra las demás sesiones. |
| Secuestro del WebSocket | Comprobación de Origin, revalidación de la sesión cada 30 s y sin tokens de API. |
| Escalada de privilegios | Permisos comprobados en el servidor en cada ruta; nadie cambia su propio rol; siempre queda un superadministrador. |
| Ver datos de otros usuarios | Las redes ajenas responden 404, no 403. |
| Abuso de la API y slowloris | 20 peticiones/s por IP, cuerpos acotados, 64 conexiones por IP y 4 096 en total, cierre tras 2 minutos sin tráfico, 10 s para el saludo TLS. |
| Tokens filtrados | Alcance, caducidad, último uso e IP visibles, revocación inmediata, solo su SHA-256 guardado. |
| Primer acceso robado | Código de un solo uso, solo en el registro del servicio. |
| Webhook hacia sitios internos | Solo lo configura un superadministrador; sin redirecciones, sin credenciales en la URL, 10 s como máximo. |

### Plugins

| Amenaza | Medida |
| --- | --- |
| Salir del aislamiento | Intérprete Wasmi sin JIT, módulo validado al cargar, sin WASI ni más importaciones que las de xbnc, funciones avanzadas de WebAssembly desactivadas. |
| Bloquear o tumbar el servidor | Combustible por evento, memoria limitada, ejecución que nunca hace crecer la pila nativa, llamadas en hilos aparte, parada tras cinco fallos. |
| Actuar sin permiso | Acciones declarativas comprobadas contra los permisos; comandos peligrosos siempre prohibidos; máximo de mensajes por evento; acceso web solo a dominios aprobados y con límites. |
| Leer datos de otros | Cada instancia solo recibe los eventos de su red; estado y almacén separados por red. |
| Fichero cambiado tras instalarlo | Se guarda su SHA-256 y no se carga si no coincide; ficheros 0600. |
| Plugin malicioso instalado | Solo un superadministrador instala, viendo permisos y huella; queda en la auditoría; catálogo remoto firmado. |

### Importación desde otro bouncer

| Amenaza | Medida |
| --- | --- |
| Fichero preparado para crear un superadministrador | La importación nunca concede ese rol. |
| Contraseñas débiles heredadas | SHA-256 con sal comparado en tiempo constante y sustituido por Argon2id; los formatos no comprobables reciben una contraseña aleatoria. |
| Rutas o tamaños abusivos | Fichero de hasta 4 MB, anidamiento limitado, nombres validados; los registros solo se leen de las carpetas indicadas. |
| Fugas en la vista previa | La vista previa no incluye secretos. |

### Actualizaciones desde el panel

| Amenaza | Medida |
| --- | --- |
| Paquete falso (servidor de descargas, DNS o red comprometidos) | Firma Ed25519 sobre el nombre del paquete y su SHA-256; la clave pública solo está en `xbnc.toml`, propiedad de root. |
| Volver a una versión con fallos conocidos | Se rechazan las versiones iguales o anteriores a la instalada. |
| Un proceso `xbnc` comprometido que intenta instalar algo como root | El ayudante (`xbnc-update.service`) no confía en nada de la carpeta del servicio: copia el paquete a `/run` (0700), comprueba la firma y la versión sobre esa copia, nunca sigue enlaces simbólicos, valida el nombre del fichero y usa cada petición una sola vez. |
| Petición de actualización falsa | Solo superadministradores, desde una sesión del panel (no con tokens de API) y con CSRF; queda en la auditoría. |
| Descargas infinitas o redirecciones a texto plano | 200 MB y 10 minutos como máximo; solo `https://`, 5 redirecciones como máximo. |

### Datos guardados y copias de seguridad

| Amenaza | Medida |
| --- | --- |
| Robo de la base de datos | Credenciales de red cifradas con XChaCha20-Poly1305; la clave fuera de la base de datos, con permisos 0600 comprobados al arrancar. |
| Robo de una copia de seguridad | Fichero 0600; incluye la clave, así que hay que guardarlo cifrado. |
| Acceso desde otros usuarios del sistema | Servicio con el usuario `xbnc` sin privilegios, carpetas 0700 y *hardening* de systemd. |

### Riesgos aceptados

- El historial se guarda sin cifrar en la base de datos: cifrarlo impediría la búsqueda y la reproducción. Lo protegen los permisos del sistema.
- Los clientes IRC no tienen segundo factor: la contraseña del bouncer es su única protección (de ahí el bloqueo por IP y Argon2id).
- Un superadministrador puede instalar un plugin con permiso para enviar mensajes en nombre de quien lo active: es una decisión de confianza que el panel muestra antes de instalar.
- Todas las conexiones a las redes salen de la IP del servidor.
- FiSH es criptografía antigua (Blowfish, sin integridad); los mensajes descifrados se guardan en el historial como el resto.

### Revisión

Cada versión revisa estas tablas: una función nueva que abra una superficie que no esté aquí se añade a ellas. Los tests que respaldan cada medida están en `crates/xbncd/tests/` y en los tests unitarios de cada módulo; el *fuzzing* cubre el parser IRC, DCC, el fichero de configuración importado, la búsqueda, y los manifiestos y la ejecución de plugins (ver [Entorno de pruebas](TESTING.md#test-environment)).

**Revisión interna de la versión 0.7.** Se revisó todo el código con estas tablas como guía (no sustituye a la auditoría externa):

- **Consultas SQL:** todas con parámetros enlazados; las partes construidas con `format!` son solo listas de columnas y cláusulas fijas.
- **Autorización de la API:** cada ruta comprueba la sesión o el token y el permiso; las rutas de redes comprueban el dueño. Corregido: `PATCH /users/{id}` respondía 404 o 403 a cuentas sin permiso según existiera el usuario.
- **Procesos externos:** el único es `dpkg` o `rpm` en el ayudante de actualización, con argumentos fijos y el paquete ya comprobado.
- **Rutas de ficheros:** registros, plugins y actualizaciones construyen rutas solo con nombres validados; el ayudante de actualización nunca sigue enlaces simbólicos.
- **Denegación de servicio:** se añadieron el límite de conexiones IRC sin autenticar por IP, los límites de conexiones y el tiempo de inactividad del panel, y el límite de cálculos de Argon2id simultáneos.
- **Plugins:** el intérprete pasó al modo de ejecución en bucle después de que el *fuzzing* encontrara que el modo de llamadas encadenadas podía agotar la pila nativa en compilaciones sin optimizar.
- **Código `unsafe`:** solo una llamada a `mallopt` al arrancar (ajuste del asignador de memoria).
- **Dependencias:** `cargo-deny` en el CI (licencias y avisos de seguridad).

**Revisión interna de la versión 1.0.** Se revisó el código nuevo de la 1.0 y se corrigieron tres puntos antes de publicar: en redes sin servicios nunca se envía la contraseña a quien use el nick «NickServ»; el intercambio de claves FiSH tiene límite de peticiones y de claves guardadas; el texto descifrado no puede romper líneas. Los ficheros de idioma se validan (marcadores, etiquetas, sin caracteres de control ni saltos de línea) y un texto que no pasa se muestra en inglés.
