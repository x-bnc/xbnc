<a id="security"></a>

# Usuarios, roles y seguridad

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

### Roles

| Rol | Para quién |
| --- | --- |
| `superadmin` | Todo, incluidos los ajustes del sistema, los plugins, las copias de seguridad, las actualizaciones, la importación y gestionar otros administradores. |
| `admin` | Gestiona usuarios (salvo administradores y superadministradores), todas las redes y la seguridad; ve el sistema. |
| `moderator` | Ve los usuarios y la auditoría, y puede suspender y reactivar usuarios normales y limitados. |
| `user` | Gestiona sus propias redes, canales e historial. Es el rol por defecto. |
| `limited` | Usa redes que le prepara un administrador; gestiona canales e historial, pero no añade, edita ni borra redes. |

### Qué puede hacer cada rol

| Permiso | superadmin | admin | moderator | user | limited |
| --- | --- | --- | --- | --- | --- |
| Editar ajustes del sistema, instalar plugins, copias de seguridad, importar, actualizar | sí | — | — | — | — |
| Ver el sistema, gestionar usuarios, seguridad y todas las redes | sí | sí | — | — | — |
| Ver usuarios y auditoría, suspender usuarios | sí | sí | sí | — | — |
| Gestionar sus propias redes | sí | sí | sí | sí | — |
| Usar sus redes, canales, historial, chat, módulos y plugins | sí | sí | sí | sí | sí |

Un administrador solo gestiona roles por debajo del suyo; nadie puede cambiarse su propio rol ni elevarse, y siempre queda al menos un superadministrador. Los usuarios no ven las redes de otros (la API responde 404).

### Cuentas

- Nombres de usuario: de 1 a 32 caracteres `A-Z a-z 0-9 . _ -`, sin distinguir mayúsculas. Contraseñas: al menos 10 caracteres.
- Cuotas por usuario: redes (`max_networks`), dispositivos (`max_clients`) y espacio de historial en MB (`max_history_mb`). Vacío: lo general del servidor.
- Suspender una cuenta cierra su acceso y desconecta sus redes. Borrarla elimina también sus redes e historial.
- Cambiar o restablecer la contraseña cierra las demás sesiones del panel de esa cuenta.

### Formas de entrar al panel

- **Usuario y contraseña**, con **verificación en dos pasos** opcional (TOTP, RFC 6238, con QR) usando Google Authenticator, Aegis, 1Password u otra compatible. El secreto se guarda cifrado; un código incorrecto cuenta como intento fallido. Los clientes IRC siguen entrando solo con contraseña: el protocolo no admite un segundo paso.
- **Passkeys** (WebAuthn): entrar con la huella, la cara o el PIN del dispositivo. Se añaden y se quitan en *Tu cuenta*.
- **OpenID Connect** (Google, Microsoft, Keycloak, Authentik…) y **LDAP**, si el administrador los configura en `xbnc.toml` (ver [Configuración](CONFIGURATION.md#configuration)). Una cuenta externa se vincula y desvincula en *Tu cuenta*.

### Medidas de seguridad

| Área | Medida |
| --- | --- |
| Contraseñas | Argon2id (m = 19 MiB, t = 2, p = 1). Un usuario que no existe cuesta lo mismo de comprobar, para no revelar qué cuentas existen. Como mucho un cálculo por CPU a la vez, para que una avalancha de intentos no agote la memoria. |
| Credenciales guardadas | Contraseñas SASL y de servidor, claves de canal, comandos al conectar, contraseñas de servicios, claves FiSH, secretos TOTP y claves de certificados, cifrados con XChaCha20-Poly1305. La clave está fuera de la base de datos (secret.key, permisos 0600 comprobados al arrancar). |
| TLS | rustls, solo TLS 1.2 y 1.3. Hacia las redes se verifica el certificado con el almacén del sistema, sin opción para desactivarlo. Un certificado de confianza (addfingerprint) solo acepta esa huella SHA-256 exacta, y se siguen comprobando las firmas del saludo TLS. |
| Texto plano | Prohibido en direcciones públicas salvo insecure = true explícito. El puerto sin TLS por defecto solo acepta la red local. |
| Fuerza bruta | Bloqueo de IP tras 5 fallos en 10 minutos durante 15 minutos (configurable), compartido entre IRC (PASS y SASL), el panel y los tokens de API. Máximo 16 conexiones IRC sin autenticar por IP. |
| Registros | Nunca se registran contraseñas, AUTHENTICATE, OPER ni mensajes a servicios (NickServ IDENTIFY, X LOGIN, Q AUTH…), en ningún nivel. |
| Entrada | Líneas limitadas a 8703 bytes; cualquier CR o LF dentro de una línea la corta, para impedir inyección de comandos; nicks, nombres y textos de módulos y plugins validados. |
| Recursos | Límites de conexiones totales, de clientes y redes por usuario; colas acotadas por cliente y hacia el servidor; tiempo máximo de registro. |
| SQL | Todas las consultas usan parámetros enlazados. |
| Privilegios | El servicio corre como xbnc sin privilegios, con aislamiento de systemd. En Docker: usuario sin privilegios, solo lectura, sin capacidades. |
| Panel: sesiones | Token aleatorio de 256 bits en cookie HttpOnly, SameSite=Strict y Secure; en la base de datos solo su SHA-256. Caducidad deslizante (12 h por defecto). |
| Panel: CSRF y origen | Token CSRF obligatorio en toda petición que cambia algo; peticiones y WebSockets con Origin de otro sitio rechazados. |
| Panel: cabeceras | CSP estricta sin scripts externos ni en línea, X-Frame-Options: DENY, nosniff, Referrer-Policy: no-referrer, HSTS con TLS y Cache-Control: no-store en la API. |
| Panel: abuso | 20 peticiones por segundo por IP, cuerpos de hasta 256 KB (4 MB para plugins), 64 conexiones por IP y 4 096 en total, cierre tras 2 minutos sin tráfico y 10 s para el saludo TLS. Con trust\_proxy se usa la IP real detrás de Nginx. |
| Primer acceso | La primera cuenta solo se crea con un código de un solo uso que el servicio escribe en su registro. |
| Secretos en la API | Las contraseñas nunca se devuelven; solo se indica si existen. |
| Tokens de API | 256 bits con prefijo xbnc\_; solo se guarda su SHA-256. Alcance, caducidad, último uso e IP visibles, revocación inmediata. |
| Módulos | Compilados dentro del programa: no se carga código externo. Op automático: \*!\*@\* se rechaza; con clave, desafío aleatorio respondido con HMAC-SHA256 y comparado en tiempo constante. |
| Registros en ficheros | Rutas construidas solo con nombres saneados (un canal no puede salir de su carpeta), carpetas 0700 y ficheros 0600. |
| Plugins | WebAssembly en el intérprete Wasmi, con combustible (CPU) por evento, 16 MB de memoria, sin acceso a ficheros ni red propia. Acciones comprobadas contra los permisos. Solo un superadministrador instala, viendo la huella SHA-256; un fichero que cambia tras instalarse no se carga. Cinco fallos seguidos lo detienen. |
| DCC | Las ofertas de IRC no pueden hacer que xBNC se conecte a redes internas ni a puertos por debajo de 1024 (salvo permiso). En tus envíos solo se conecta de vuelta a la IP de tu dispositivo. Cada puerto acepta una sola conexión, con tiempo de espera, límite de tamaño y cierre por inactividad. |
| Login en servicios | En redes sin NickServ (IRCnet, EFnet) el modo automático nunca envía la contraseña. |
| Traducciones | Los ficheros de idioma son datos, no código: un texto de IRC con saltos de línea o caracteres de control, o con marcadores distintos de los del inglés, se ignora. Ficheros de 2 MB como máximo y 100 idiomas. |
| FiSH | El intercambio de claves admite 5 peticiones por minuto y 100 claves por sesión; el texto descifrado no puede romper líneas. |
| Webhooks | Solo http(s), sin usuario en la URL ni caracteres de control, certificado verificado, sin seguir redirecciones, 10 s como máximo. |
| Importación | Solo un superadministrador; nunca concede ese rol; la vista previa no devuelve secretos. |
| Auditoría | Cada acción del panel, de IRC y de la terminal queda registrada con fecha, autor y detalle (Seguridad → Auditoría, o xbncd audit). |
| Fuzzing | Parser IRC, DCC, importador, búsqueda, manifiestos y ejecución de plugins, cada semana en el CI con los fallos anteriores como regresiones. |

### Recomendaciones

- Usa un certificado real (Let's Encrypt) y no publiques puertos en texto plano.
- Guarda las copias de seguridad cifradas: contienen la clave secreta.
- Todas las conexiones a las redes salen de la IP del servidor: si un usuario abusa en una red, la IP del servidor puede acabar bloqueada.
- Activa la verificación en dos pasos o una passkey en las cuentas de administración.

Para avisar de una vulnerabilidad, sigue `SECURITY.md`: no la publiques; usa el aviso privado del repositorio. Respuesta en 3 días como máximo, evaluación en 7 y corrección en 90 como mucho. El modelo de amenazas completo está en [Modelo de amenazas](THREAT_MODEL.md#threat-model).
