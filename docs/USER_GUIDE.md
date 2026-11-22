# Guía de Omnyra

Omnyra reúne tus redes en una app de escritorio. Todo se guarda en tu ordenador; las contraseñas y los tokens van al llavero del sistema, nunca a un archivo.

## 1. Primeros pasos

1. **Instala** Omnyra (ver el README) y, en Windows, FFmpeg.
2. Abre **Ajustes → Cuentas conectadas** y pulsa **Conectar** en cada red:
   - **YouTube**: se abre el navegador; elige el canal (personal o de marca) y acepta.
   - **Twitch**: Omnyra muestra un código; se abre twitch.tv/activate para escribirlo.
   - **Instagram**: necesitas una cuenta profesional (creador o empresa).
   - **TikTok**: mientras la app no esté auditada por TikTok, solo publica en privado.
3. La primera sincronización trae el histórico (YouTube puede tardar un poco). Después Omnyra sincroniza sola cada pocas horas.

En **Ajustes → Preferencias** puedes elegir que siga en la bandeja al cerrar la ventana, que arranque con el sistema y que avise con notificaciones.

## 2. Analíticas

- **Panel**: suscriptores, visualizaciones, horas vistas y publicaciones de todas tus redes, con el periodo arriba (30 días, 1 año, 5 años, todo).
- **Vista de cada cuenta** (barra lateral): cifras, gráficas, contenidos y, en YouTube, el mapa de **mejor hora para publicar** (necesita al menos 15 vídeos).
- La **campana** avisa de hitos de seguidores y de vídeos que van mucho mejor (o peor) de lo normal.

## 3. Planificar

- **Ideas**: tablero Idea → Guion → Grabado → Editado → Publicado. Arrastra las tarjetas; adjunta archivos (se enlazan, no se copian).
- **Calendario**: publicaciones, programaciones, recordatorios y fechas de ideas. Arrastra para mover; los recordatorios avisan con una notificación.

## 4. Publicar

- **Subidas**: un vídeo a una cuenta, con su título, descripción, etiquetas y (en YouTube) miniatura y lista.
- **Publicar en varias redes**: elige el vídeo una vez, añade destinos y ajusta cada uno (texto propio, formato 16:9 / 9:16 / 1:1, recorte o fondo difuminado, inicio y fin, hora programada). Omnyra prepara cada versión con FFmpeg y la sube en segundo plano. Para programar, Omnyra tiene que estar abierta o en la bandeja a esa hora.
- **Preparar y descargar**: para redes sin subida por API (o si prefieres hacerlo a mano), guarda el archivo en el formato elegido y copia el texto.

YouTube y TikTok suben en **privado** hasta que sus APIs aprueben la app; luego puedes cambiar la visibilidad desde su web.

## 5. Comunidad

- **Comunidad** junta los comentarios de YouTube e Instagram. Filtra por red, cuenta, estado, categoría o texto.
- Cada comentario se clasifica solo (pregunta, positivo, negativo, spam); puedes cambiarlo a mano y se respeta.
- **Preguntas repetidas**: grupos de preguntas parecidas. "Enviar al tablero de ideas" crea una idea con todas ellas.
- Para **responder** la primera vez, Omnyra pide un permiso más a la red.
- Los comentarios con **posible estafa** salen marcados en rojo con el motivo (enlace que imita a una marca, sorteo falso, "escríbeme por Telegram"…).

## 6. Crear

- **Miniaturas**: capas de imagen, texto con contorno y sombra, formas y stickers. Guarda plantillas y exporta al tamaño de cada red; "Usar en una subida" la lleva directamente al formulario.
- **Biblioteca**: importa tus skins (por ejemplo normal, Halloween y Navidad: usa el campo *variante*), avatares y fondos.
- **Textos**: títulos, descripciones y hashtags a partir de plantillas por red. Opcional: pega tu clave de API de Anthropic para pedir sugerencias a Claude; antes de enviar ves exactamente qué se manda.
- **Momentos para shorts**: elige un vídeo largo de YouTube (usa la retención) o un directo de Twitch (usa el chat que Omnyra grabó mientras estaba abierta), elige el archivo de vídeo de tu disco y recorta en vertical los mejores fragmentos.

## 7. Patrocinios y media kit

- Registra cada acuerdo: marca, contacto, entregables, fechas, importe y estado del pago. Omnyra marca lo vencido y suma lo firmado, cobrado y pendiente por divisa.
- **Media kit**: elige cuentas y cifras, y guárdalo como HTML o "Imprimir → Guardar como PDF".

## 8. Competencia

Sigue canales públicos de YouTube (por @nombre o enlace) y compáralos contigo: suscriptores, crecimiento en 30 días, vídeos por semana y vistas medianas. **Tendencias**: los vídeos más vistos de la semana para tus palabras clave (cada búsqueda gasta cuota de YouTube, así que se hace como mucho una vez al día).

## 9. Seguridad

- **Revisión de cuentas**: lista por red (verificación en dos pasos, recuperación, apps con acceso…) con enlaces a sus ajustes. Márcalo según lo vayas revisando.
- **Suplantación**: busca cuentas con un nombre parecido al tuyo entre quienes comentan o en YouTube.
- **Bloqueo con PIN**: Omnyra pide el PIN al abrirse y tras un rato sin usarla. Protege la interfaz; la base de datos de tu disco no se cifra.
- **Registro de acciones**: conexiones, respuestas y cambios del bloqueo, con una comprobación de que nadie lo ha modificado.

## 10. Preguntas frecuentes

- **"No hay llavero disponible"** (Linux): instala y abre GNOME Keyring, KWallet o KeePassXC con Secret Service.
- **"Cuota agotada"** (YouTube): la cuota diaria se reinicia a medianoche de California; las subidas tienen su propio cupo.
- **No veo el icono de la bandeja** (Linux): tu escritorio necesita soporte de StatusNotifierItem (en GNOME, la extensión AppIndicator).
- **Cambié de ordenador**: copia la carpeta de datos de Omnyra; tendrás que volver a conectar las cuentas porque los tokens están en el llavero del sistema antiguo.
