<a id="performance"></a>

# Rendimiento

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Prueba de carga (`cargo run --release --example loadtest -- --users 1000 --clients 1 --rate 2 --seconds 60`): un servidor IRC simulado envía mensajes a todas las redes a la vez (el peor caso) y cada cliente mide cuánto tarda en recibirlos, pasando por el historial, los módulos y los plugins. Máquina: 2 vCPU Xeon a 2,1 GHz, 7 GB de RAM, disco en red, configuración por defecto (SQLite).

| Usuarios y redes | Clientes | Mensajes/s entregados | CPU (de un núcleo) | Memoria | Latencia p50 / p95 / p99 | Todas las redes de vuelta tras una caída |
| --- | --- | --- | --- | --- | --- | --- |
| 500 | 500 | 492 | 5 % | 52 MB | 21 / 43 / 46 ms | 6,2 s |
| 500 | 500 | 4 847 | 43 % | 56 MB | 46 / 64 / 72 ms | 6,2 s |
| 1 000 | 1 000 | 1 951 | 17 % | 88 MB | 50 / 80 / 95 ms | 6,2 s |
| 1 000 | 2 000 | 1 968 | 11 % | 114 MB | 44 / 66 / 78 ms | 6,2 s |
| 2 000 | 2 000 | 1 968 | 19 % | 153 MB | 75 / 141 / 155 ms | 6,3 s |

- Ningún mensaje se perdió en ninguna prueba.
- La latencia sube con el tamaño de la ráfaga: con 2 000 redes recibiendo el mismo mensaje a la vez, el último se entrega unos 150 ms después que el primero. Con tráfico real, repartido en el tiempo, es de pocos milisegundos.
- Memoria: unos 45 KB por red conectada y unos 30 KB más por cliente, sin contar el historial (en disco).
- Los logins de clientes (Argon2id) van de uno en uno por CPU: 2 000 clientes entrando juntos tardan unos 27 s en 2 núcleos, sin disparar la memoria.
- Disco: unos 150 bytes por mensaje guardado, con el índice de búsqueda.

**Para dimensionar:** un servidor pequeño (1–2 vCPU, 1 GB de RAM) atiende cientos de usuarios con varias redes y miles de mensajes por segundo. Con muchos usuarios, revisa `limits.max_connections`, `LimitNOFILE` del servicio (65 536 por defecto) y la retención del historial.

### Cómo se mide

La prueba (`crates/xbncd/examples/loadtest.rs`) arranca en un solo proceso:

- un servidor IRC simulado con un canal `#xbnc`;
- xBNC con N usuarios, cada uno con su red conectada a ese servidor y dentro de `#xbnc`;
- uno o varios clientes IRC por usuario conectados a xBNC.

El servidor envía `--rate` mensajes por segundo al canal, **a todas las redes a la vez** (el peor caso: ráfagas simultáneas). Cada mensaje lleva la hora de envío y cada cliente mide cuánto tarda en llegar: servidor IRC → xBNC (guardado en el historial, módulos, plugins) → cliente. Al final el servidor corta todas las conexiones y la prueba mide cuánto tardan todas las redes en volver.

La memoria y la CPU son las del proceso completo, así que incluyen el servidor simulado y los clientes de la prueba (xBNC solo consume algo menos). La compilación es `--release` con la configuración por defecto (historial en SQLite con WAL).

### Problemas encontrados y corregidos con esta prueba

| Problema | Antes | Después |
| --- | --- | --- |
| Memoria tras muchas entradas simultáneas: glibc guardaba en cada hilo los 19 MB de cada cálculo de Argon2id. | 1.680 MB con 500 clientes | 55 MB (`util::tune_allocator`, umbral fijo de `mmap`) |
| Cientos de cálculos de Argon2id a la vez (también útil contra avalanchas de intentos de entrada). | Sin límite | Uno por CPU (`crypto::with_hash_slot`) |
| Una transacción de SQLite por mensaje guardado. | p50 70 ms y 13 % de CPU con 500 redes | p50 21 ms y 5 % de CPU: un único hilo escritor guarda en una sola transacción todo lo que espera (`Db::insert_message`) |

### Pendiente

- Repetir la prueba en hardware dedicado y con varias redes por usuario.
- Medir con servidores IRC reales a distancia (la prueba usa uno local, sin latencia de red).

Para repetirla: `cargo run --release --example loadtest -- --users 2000 --clients 1 --rate 1 --seconds 60`. Las opciones son `--users`, `--clients` (por usuario), `--rate` (mensajes por segundo al canal) y `--seconds`.
