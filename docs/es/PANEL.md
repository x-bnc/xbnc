# Panel web, historial y avisos

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Panel web](#panel)
- [Historial y búsqueda](#history)
- [Avisos](#notifications)

<a id="panel"></a>

## Panel web

| Página | Para qué sirve |
| --- | --- |
| Inicio | Estado en vivo de tus redes (se actualiza cada 2 segundos) y, para administradores, memoria, CPU, tráfico, conexiones, IPs bloqueadas, alertas activas y aviso de versiones nuevas. |
| Chat | Cliente IRC web: canales y privados de todas tus redes, con historial. El panel cuenta como uno de tus dispositivos. |
| Menciones | Todo lo que te han dicho en privado o donde han escrito tu nick, de más nuevo a más antiguo. |
| Redes | Añadir (asistente de cuatro pasos) y editar redes; control (conectar, desconectar, reconectar); canales; dispositivos conectados (con botón para desconectar uno); historial con búsqueda y exportación; comandos al conectar; certificado de cliente; tarjeta «Conecta tu cliente IRC». |
| Módulos | Módulos y plugins de cada red. Selector de red que sirve con cualquier número de redes y «Copiar a otras redes» (módulos y plugins, salvo el certificado). |
| Plugins | Catálogo oficial y remoto, instalación con un clic o subiendo un .wasm, permisos y sitios aprobados, activar o desactivar en el servidor, desinstalar, registro en directo y modo desarrollo (superadministradores). |
| Usuarios | Altas, roles, cuotas de redes, dispositivos e historial, suspensión, restablecer contraseña, uso de cada cuenta e importación desde otro bouncer. |
| Seguridad | Intentos de acceso (IRC, panel y tokens), IPs bloqueadas con botón para desbloquear, sesiones abiertas y auditoría de todas las acciones. |
| Ajustes | Historial, límites, protección del acceso, idioma por defecto, alertas, copias de seguridad, actualizaciones y reinicio del servicio. |
| Tu cuenta | Foto de perfil (se recorta a 256×256 en el navegador; PNG, JPEG o WebP de hasta 256 KB; la ves tú y los administradores), idioma, zona horaria, avisos, contraseña, verificación en dos pasos, passkeys, cuentas externas vinculadas, notas, tokens de API, informe para pruebas y sesiones abiertas. |

### Ajustes en caliente

Lo que se cambia en *Ajustes* se guarda en la base de datos, se aplica sin reiniciar y tiene prioridad sobre `xbnc.toml`. El icono ↺ junto a un campo vuelve al valor de `xbnc.toml`.

| Grupo | Ajustes | Por defecto |
| --- | --- | --- |
| Historial | Días de retención (0: para siempre); mensajes reproducidos al reconectar; mensajes para un dispositivo nuevo; límite de CHATHISTORY. | 30 días; 1000; 100; 500 |
| Límites | Conexiones totales; dispositivos por usuario; redes por usuario; cola por cliente (líneas antes de desconectar a un cliente lento). Cada usuario puede tener excepciones en Usuarios. | 1000; 10; 10; 4096 |
| Protección del acceso | Intentos fallidos antes de bloquear; ventana; duración del bloqueo; tiempo para completar el login. | 5; 600 s; 900 s; 30 s |
| General | Idioma por defecto para quien no ha elegido uno en Tu cuenta. Se guarda y se aplica al pulsarlo, también a quien lo cambia. | el de xbnc.toml |
| Alertas | Ver [Monitorización](ADMINISTRATION.md#monitoring). |   |
| Copias de seguridad | Automáticas sí/no, cada cuántas horas (1–720) y cuántas conservar (1–365). | no; 24 h; 7 |

Lo que se lee al arrancar (puertos, certificados, base de datos, clave secreta, el propio panel y el nivel de registro) se cambia en `xbnc.toml` y necesita `sudo systemctl restart xbnc`. La página lo indica. Los superadministradores tienen además un botón para reiniciar el servicio.

### Accesibilidad y uso en el móvil

- Navegable con teclado, con foco visible; los diálogos se cierran con Escape.
- Respeta «reducir movimiento» del sistema.
- En pantallas estrechas la barra lateral se abre con el botón de menú.
- El tema (claro, u oscuro en negro, amarillo y gris) sigue al sistema la primera vez; el botón de luna o sol lo cambia y lo recuerda en ese navegador.
- Se puede instalar como aplicación (PWA) para tener las notificaciones push.
- En inglés y en español, y en cualquier idioma instalado como fichero de traducción (ver [Traducir xBNC](TRANSLATIONS.md#translations)). El idioma se cambia en la pantalla de entrada, en *Tu cuenta* o (el predeterminado del servidor) en *Ajustes*, y se aplica al momento al panel y a los mensajes de xBNC en IRC, también con el cliente conectado. Con `/msg *status language en` se cambia desde IRC.

<a id="history"></a>

## Historial y búsqueda

xBNC guarda los mensajes de canales y privados (y, con el módulo [Eventos](MODULES.md#mod-events), entradas, salidas, kicks, cambios de nick, temas y modos). Cada mensaje lleva la hora del servidor si la red la envía.

### Buscar

En *Redes → (red) → Historial*, o con la API. La búsqueda usa un índice de texto completo:

- Encuentra palabras enteras y comienzos de palabra: `desplie` encuentra «despliegue».
- No distingue mayúsculas ni tildes: `cancion` encuentra «Canción».
- Varias palabras tienen que aparecer todas, en cualquier orden. Entre comillas se busca la frase exacta: `"fuera de servicio"`.
- Si no hay coincidencias por palabra, busca también trozos del medio de una palabra: `spliegue` encuentra «despliegue».
- Se combina con los filtros de conversación, emisor y fechas.

### Exportar y borrar

Se exporta en TXT o JSON (todo, o una conversación). Se borra una conversación o todo el historial de la red desde el panel, con `/msg *status clearbuffer` (o `clearallbuffers`, `clearallchannelbuffers`, `clearallquerybuffers`) o con la API.

### Retención y cuotas

- **General:** *Ajustes → Historial*, días de retención (30 por defecto; 0, para siempre).
- **Por canal:** en los canales de cada red, o con `/msg *status setbuffer`: lo del servidor, no guardar nada, para siempre o N días.
- **Por usuario:** un administrador puede fijar una cuota de espacio de historial (`max_history_mb`) en *Usuarios*; el uso se ve en la ficha de cada usuario.
- Espacio: unos 150 bytes por mensaje guardado, con el índice de búsqueda.

### Registros en ficheros

Aparte del historial, el módulo [Registros en ficheros](MODULES.md#mod-log) escribe ficheros de texto diarios con el formato clásico de IRC en `/var/lib/xbnc/logs/usuario/red/#canal/AAAA-MM-DD.log`, con la hora en tu zona horaria.

<a id="notifications"></a>

## Avisos

En *Tu cuenta → Avisos*, xBNC te avisa de las menciones y los privados.

| Opción | Qué hace | Por defecto |
| --- | --- | --- |
| Activar avisos | Interruptor general. | no |
| Solo cuando no tengo ningún cliente conectado | No avisa si tienes un cliente IRC o el chat del panel abiertos. | sí |
| Cuando alguien escribe mi nick en un canal | Menciones. | sí |
| Cuando recibo un mensaje privado | Privados. | sí |

| Canal | Qué necesitas |
| --- | --- |
| Navegador y móvil (push) | Pulsa «Activar en este dispositivo» desde el panel abierto en ese dispositivo. En el móvil, instala antes el panel como aplicación. Se puede activar en varios dispositivos. |
| Telegram | Crea un bot con @BotFather, escríbele una vez y pon su token y tu id de chat (te lo dice @userinfobot). El panel no vuelve a mostrar el token completo. |
| Discord | Un webhook del canal: Ajustes del canal → Integraciones → Webhooks. Los avisos no mencionan a nadie. |
| Correo | Tu dirección. El administrador tiene que configurar la sección \[smtp\] de xbnc.toml. |
| Dirección web propia | Una URL https que recibe un POST con el aviso en JSON. |

«Enviar un aviso de prueba» manda uno a todos los canales configurados. Para no inundarte, xBNC agrupa los avisos: si llegan muchos seguidos, los que no se envían se cuentan y el siguiente aviso indica cuántos se saltaron.

### Formato del aviso en la dirección web propia

```json
{
  "network": "undernet",
  "network_id": 1,
  "conversation": "#xbnc",
  "from": "bob",
  "text": "y2k, ¿mañana a qué hora es la reunión?",
  "private": false,
  "ts": 1762851240000,
  "skipped": 0
}
```

En push, al tocar el aviso se abre el chat del panel en esa conversación.
