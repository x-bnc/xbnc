<a id="ircv3"></a>

# IRCv3

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

xBNC negocia con cada servidor las capacidades que ofrece y le interesan, y ofrece a cada cliente las suyas. A cada cliente solo le llega lo que ha pedido: un cliente clásico recibe los mensajes de siempre (por ejemplo, el JOIN corto y solo el rango más alto en la lista de usuarios).

### Con los servidores

| Capacidad | Para qué |
| --- | --- |
| `server-time` | Hora real de cada mensaje. |
| `message-tags` | Etiquetas en los mensajes. |
| `multi-prefix` | Todos los rangos de un usuario (@+), no solo el más alto. |
| `extended-join` | Las entradas llevan la cuenta y el nombre real. |
| `away-notify` | Avisos cuando alguien se marca como ausente o vuelve. |
| `account-notify` | Avisos cuando alguien se identifica o deja de estarlo. |
| `account-tag` | Cada mensaje indica la cuenta de quien lo envía. |
| `chghost` | Cambios de usuario o host sin salir y entrar. |
| `userhost-in-names` | La lista de usuarios con nick!usuario@host. |
| `invite-notify` | Invitaciones a otras personas en tus canales. |
| `setname` | Cambios de nombre real. |
| `cap-notify` | El servidor añade o quita capacidades en caliente. |
| `sasl` | Identificación SASL (PLAIN y EXTERNAL hacia el servidor; PLAIN desde los clientes). |

### Con los clientes

| Capacidad | Para qué |
| --- | --- |
| `server-time` | Hora real de cada mensaje. |
| `message-tags` | Etiquetas en los mensajes. |
| `echo-message` | Tus propios mensajes vuelven confirmados por el servidor. |
| `batch` | Grupos de mensajes (historial, reproducción). |
| `sasl` | Identificación SASL (PLAIN y EXTERNAL hacia el servidor; PLAIN desde los clientes). |
| `draft/chathistory` | El cliente pide el historial que necesita (CHATHISTORY). |
| `draft/event-playback` | El historial incluye entradas, salidas, kicks y cambios de nick como eventos reales. |
| `draft/read-marker` | Marcadores de lectura compartidos entre dispositivos. |
| `soju.im/bouncer-networks` | Todas tus redes en una sola conexión. |
| `soju.im/bouncer-networks-notify` | Avisos de cambios en tus redes. |
| `extended-join` | Las entradas llevan la cuenta y el nombre real. |
| `away-notify` | Avisos cuando alguien se marca como ausente o vuelve. |
| `account-notify` | Avisos cuando alguien se identifica o deja de estarlo. |
| `account-tag` | Cada mensaje indica la cuenta de quien lo envía. |
| `chghost` | Cambios de usuario o host sin salir y entrar. |
| `multi-prefix` | Todos los rangos de un usuario (@+), no solo el más alto. |
| `userhost-in-names` | La lista de usuarios con nick!usuario@host. |
| `invite-notify` | Invitaciones a otras personas en tus canales. |
| `setname` | Cambios de nombre real. |
| `cap-notify` | El servidor añade o quita capacidades en caliente. |

Con `CAP LS 302` se anuncia `sasl=PLAIN`. Si el servidor no admite una capacidad que pide un cliente, xBNC la ofrece igual cuando puede construirla él (por ejemplo, el JOIN extendido con los datos que conoce). El informe para pruebas recoge, por red, lo que ofrece el servidor (`caps_offered`), lo que xBNC activó (`caps_acked`) y lo que pidió cada cliente.
