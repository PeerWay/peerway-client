(() => {
  'use strict';

  // Lightweight, dependency-free Spanish/English UI translation. Spanish is the
  // source language in the HTML; unknown strings fall back to Spanish.
  const DICT = {
    'Enviar': 'Send',
    'Recibir': 'Receive',
    'Nube': 'Cloud',
    'Ajustes': 'Settings',
    'Transferir': 'Transfer',
    'Actividad': 'Activity',
    'En curso': 'In progress',
    'Solicitudes': 'Requests',
    'Equipos cerca de ti': 'Devices near you',
    'Buscando equipos en tu red…': 'Looking for devices on your network…',
    'Elige un equipo o escribe su código.': 'Pick a device or enter its code.',
    'Compártelo para que te envíen archivos.': 'Share it so others can send you files.',
    'No se suben a la nube.': 'They are not uploaded to the cloud.',
    'Abrir descargas': 'Open downloads',
    'Recibidos recientemente': 'Recently received',
    'Abrir archivo': 'Open file',
    'Mostrar en carpeta': 'Show in folder',
    'Notificaciones, servicio y contactos': 'Notifications, service and contacts',
    '¿Ayudar a mejorar PeerWay?': 'Help improve PeerWay?',
    'Estadísticas de uso con un identificador aleatorio de instalación. Incluye versión, plataforma, plan, si tienes sesión iniciada y contadores; no incluye archivos, nombres, códigos ni direcciones IP. Puedes cambiarlo en Ajustes.': 'Usage statistics with a random installation identifier. It includes version, platform, plan, whether you are signed in, and counters; it does not include files, names, codes or IP addresses. You can change this in Settings.',
    'Aceptar': 'Accept',
    'Política de privacidad': 'Privacy policy',
    'Modo temporal': 'Temporary mode',
    'Inicia sesión para conservar tu código': 'Sign in to keep your code',
    'Conectando…': 'Connecting…',
    'PeerWay conectado': 'PeerWay connected',
    'Sin conexión': 'Offline',
    'Conectado': 'Connected',
    'Comprobando…': 'Checking…',
    'Cifrada': 'Encrypted',
    'Servicio': 'Service',
    'Conexión': 'Connection',
    'Tu código': 'Your code',
    'Copiar mi código': 'Copy my code',
    'Copiar código': 'Copy code',
    'Copiar enlace': 'Copy link',
    'Abrir descargas': 'Open downloads',
    'Enviar archivos': 'Send files',
    'Directo, cifrado y sin subir nada a un servidor.': 'Direct, encrypted, with nothing uploaded to a server.',
    'Recibir archivos': 'Receive files',
    'Comparte tu código. Los demás se conectan a ti.': 'Share your code. Others connect to you.',
    'Nube PeerWay': 'PeerWay Cloud',
    'Opcional. Tus archivos disponibles en cualquier lugar.': 'Optional. Your files available anywhere.',
    'Controla tu identidad, red y servidor de coordinación.': 'Manage your identity, network and coordination service.',
    'Estás en modo temporal.': 'You are in temporary mode.',
    'Tu código cambia si cierras PeerWay. Crea una cuenta gratis para conservarlo y desbloquear la nube.': 'Your code changes if you close PeerWay. Create a free account to keep it and unlock the cloud.',
    'Crear cuenta': 'Create account',
    'Crear cuenta gratis': 'Create free account',
    'Iniciar sesión': 'Sign in',
    'Cerrar sesión': 'Sign out',
    'Nueva transferencia': 'New transfer',
    'Escribe el código del otro equipo y elige los archivos.': "Enter the other device's code and choose the files.",
    'Cifrado': 'Encrypted',
    'Tus archivos van cifrados.': 'Your files are encrypted.',
    'Código del destinatario': 'Recipient code',
    'Destinatarios': 'Recipients',
    'Añadir': 'Add',
    'Archivos': 'Files',
    'Suelta archivos aquí': 'Drop files here',
    'o': 'or',
    'selecciona desde tu equipo': 'choose from your device',
    'Los archivos viajan directamente. No se suben a la nube.': 'Files travel directly. They are not uploaded to the cloud.',
    'Quitar todos': 'Remove all',
    'Cómo llega': 'How it gets there',
    '1. Misma red': '1. Same network',
    'La conexión más rápida, sin internet.': 'The fastest connection, without internet.',
    '2. Internet directa': '2. Direct internet',
    'Los equipos se conectan entre ellos.': 'Devices connect to each other.',
    '3. Último recurso': '3. Last resort',
    'Un puente cifrado que no puede leer tus archivos.': 'An encrypted bridge that cannot read your files.',
    'Buscando equipos…': 'Looking for devices…',
    'El descubrimiento LAN está activo.': 'LAN discovery is active.',
    'Descubrimiento LAN desactivado.': 'LAN discovery is off.',
    'Transferencias activas': 'Active transfers',
    'Actualizar': 'Refresh',
    'No hay envíos en curso.': 'No sends in progress.',
    'TU CÓDIGO': 'YOUR CODE',
    'Comparte este código con quien te va a enviar archivos. No necesitas abrir puertos ni configurar nada.': 'Share this code with the person sending you files. You do not need to open ports or configure anything.',
    'Funciona en cualquier red': 'Works on any network',
    'Siempre llega': 'Always arrives',
    'Solicitudes pendientes': 'Pending requests',
    'Acepta o rechaza cada envío.': 'Accept or decline each send.',
    'No hay solicitudes pendientes.': 'No pending requests.',
    'Equipos en tu red': 'Devices on your network',
    'Descubiertos por UDP, sin internet.': 'Discovered via UDP, without internet.',
    'No se han encontrado equipos todavía.': 'No devices found yet.',
    'Historial': 'History',
    'Borrar historial': 'Clear history',
    'Nombre visible': 'Display name',
    'Guardar': 'Save',
    'Privacidad': 'Privacy',
    'Términos': 'Terms',
    'Ayuda': 'Help',
    'Recordar este equipo': 'Remember this device',
    'Añadir este equipo a contactos de confianza': 'Add this device to trusted contacts',
    'Confiar en este equipo': 'Trust this device',
    'Confiar': 'Trust',
    'Añadir a contactos de confianza': 'Add to trusted contacts',
    'Equipo agregado a contactos de confianza.': 'Device added to trusted contacts.',
    'Recepción aceptada. Este equipo queda recordado.': 'Transfer accepted. This device is now remembered.',
    '¿Olvidaste tu contraseña?': 'Forgot your password?',
    'Escribe tu correo y vuelve a pulsar «¿Olvidaste tu contraseña?».': 'Write your email and press “Forgot your password?” again.',
    'Correo': 'Email',
    'Te enviaremos un enlace para confirmarlo. Para la nube hace falta confirmarlo.': 'We will email you a link to confirm it. The cloud needs it confirmed.',
    'Enviar enlace': 'Send link',
    'Sin correo confirmado.': 'No confirmed email.',
    'Sin correo. Añade uno para usar la nube.': 'No email yet. Add one to use the cloud.',
    'Correo confirmado.': 'Email confirmed.',
    'Correo sin confirmar. Revisa tu bandeja.': 'Email not confirmed yet. Check your inbox.',
    'tu@correo.com': 'you@example.com',
    'Tu nombre': 'Your name',
    'Tu historial aparecerá aquí.': 'Your history will appear here.',
    'NUBE PEERWAY': 'PEERWAY CLOUD',
    'Tu código permanente.': 'Your permanent code.',
    'Tus archivos, en cualquier lugar.': 'Your files, anywhere.',
    'Transferir es gratis. La nube es opcional: durante la beta gratuita incluye 5 GB en total y cada archivo se elimina 30 días después de la subida.': 'Transferring is free. The cloud is optional: during the free beta it includes 5 GB in total and each file is deleted 30 days after upload.',
    'El P2P es gratis. La nube también durante la beta.': 'P2P is free. So is the cloud during the beta.',
    'Beta gratuita: 5 GB en total; cada archivo se elimina 30 días después de la subida.': 'Free beta: 5 GB in total; each file is deleted 30 days after upload.',
    'Beta gratuita · 5 GB en total · borrado a los 30 días': 'Free beta · 5 GB in total · deleted after 30 days',
    '5 GB gratis · borrado a los 30 días': '5 GB free · deleted after 30 days',
    'gratis en la nube': 'free in the cloud',
    'Durante la beta gratuita, tu cuenta incluye': 'During the free beta, your account includes',
    'Tu cuenta Gratis incluye': 'Your Free account includes',
    'de almacenamiento en total. Cada archivo se elimina automáticamente': 'of storage in total. Each file is automatically deleted',
    '30 días': '30 days',
    'después de la subida.': 'after upload.',
    'incluidos': 'included',
    'Borrado': 'Deleted',
    'después de subir': 'after upload',
    'Transferencias sin límite': 'Unlimited transfers',
    'Sin tarjeta ni suscripción': 'No card or subscription',
    'PLAN PRO': 'PRO PLAN',
    'Más espacio para tus archivos': 'More space for your files',
    'Amplía tu nube y conserva los archivos mientras el plan esté activo.': 'Expand your cloud and keep files while the plan is active.',
    'Los planes de pago no están disponibles durante la beta gratuita.': 'Paid plans are not available during the free beta.',
    'TU NUBE': 'YOUR CLOUD',
    'Iniciar con el equipo': 'Start with the computer',
    'Se abre en la bandeja al iniciar sesión.': 'Opens in the tray when you sign in.',
    '/ 30 días': '/ 30 days',
    'Beta gratuita': 'Free beta',
    'Código permanente y 5 GB de nube gratis.': 'Permanent code and 5 GB of free cloud.',
    'Usuario': 'Username',
    'Contraseña': 'Password',
    'usuario': 'username',
    'mínimo 8 caracteres': 'at least 8 characters',
    'Tu identidad criptográfica se crea en este equipo. La contraseña solo protege tu cuenta.': 'Your cryptographic identity is created on this device. The password only protects your account.',
    'Inicia sesión para recuperar tu código permanente y tu nube.': 'Sign in to recover your permanent code and cloud.',
    'Cifrado en tránsito': 'Encrypted in transit',
    'Sube, lista y descarga desde la interfaz o la API.': 'Upload, list and download from the app or API.',
    'Código permanente': 'Permanent code',
    'Tu código no cambia entre reinicios ni equipos.': 'Your code does not change across restarts or devices.',
    'Plan Gratis': 'Free plan',
    'Gratis': 'Free',
    'PLAN GRATIS': 'FREE PLAN',
    'BETA GRATUITA': 'FREE BETA',
    ', código permanente y acceso desde cualquier red.': ', a permanent code and access from any network.',
    'de almacenamiento': 'of storage',
    'Descargas HTTPS': 'HTTPS downloads',
    'Sin límite de velocidad': 'No speed limit',
    'Pago con PayPal. Pago único por 30 días; sin renovación automática.': 'Pay with PayPal. One-time payment for 30 days; no automatic renewal.',
    'o activa una licencia': 'or activate a license',
    'Activar licencia': 'Activate license',
    'La nube se activa al instante. Transferir sigue siendo gratis.': 'The cloud activates instantly. Transferring remains free.',
    'Almacenamiento': 'Storage',
    'Subir archivo': 'Upload file',
    'Archivos en la nube': 'Files in the cloud',
    'Tu nube está vacía.': 'Your cloud is empty.',
    'Este equipo': 'This device',
    'Nombre visible y carpeta de descargas.': 'Display name and downloads folder.',
    'Nombre del equipo': 'Device name',
    'Carpeta de descargas': 'Downloads folder',
    'Aceptar automáticamente contactos de confianza': 'Automatically accept trusted contacts',
    'Solo para códigos que agregues como confiables.': 'Only for codes you add as trusted.',
    'Descubrimiento en la red local': 'Local network discovery',
    'Permite encontrar otros equipos cercanos.': 'Finds other nearby devices.',
    'Buscar actualizaciones automáticamente': 'Check for updates automatically',
    'Se buscan en segundo plano. Tú decides cuándo instalarlas.': 'Checks in the background. You decide when to install them.',
    'Compartir estadísticas de uso': 'Share usage statistics',
    'Usa un identificador aleatorio de instalación. Incluye versión, plataforma, plan, si tienes sesión iniciada y contadores; no incluye archivos, nombres, códigos ni direcciones IP.': 'Uses a random installation identifier. It includes version, platform, plan, whether you are signed in, and counters; it does not include files, names, codes or IP addresses.',
    'Notificaciones': 'Notifications',
    'Avisos dentro de la app.': 'Alerts inside the app.',
    'Notificaciones en la app': 'In-app notifications',
    'Actualizaciones, transferencias y avisos importantes.': 'Updates, transfers and important alerts.',
    'Actualizaciones': 'Updates',
    'Comprueba si hay una versión nueva. Tú decides cuándo instalarla.': 'Check whether a new version is available. You decide when to install it.',
    'Listo para comprobar.': 'Ready to check.',
    'Actualización de seguridad': 'Security update',
    'Actualización de seguridad disponible.': 'Security update available.',
    'Cuando haya una versión nueva o falle la actualización.': 'When a new version is available or an update fails.',
    'Transferencias': 'Transfers',
    'Cuando termine o falle una transferencia.': 'When a transfer completes or fails.',
    'Limpiar': 'Clear',
    'No hay notificaciones.': 'No notifications.',
    'TU PRIVACIDAD': 'YOUR PRIVACY',
    '¿Compartir estadísticas de uso?': 'Share usage statistics?',
    'Estadísticas de uso con un identificador aleatorio de instalación. Incluye versión, plataforma, plan, si tienes sesión iniciada y contadores; no incluye archivos, nombres, códigos ni direcciones IP. Puedes cambiarlo en Ajustes.': 'Usage statistics with a random installation identifier. It includes version, platform, plan, whether you are signed in, and counters; it does not include files, names, codes or IP addresses. You can change this in Settings.',
    'Puedes cambiarlo cuando quieras en Ajustes.': 'You can change it anytime in Settings.',
    'No se usa para publicidad ni se vende.': 'It is not used for advertising or sold.',
    'Ahora no': 'Not now',
    'Aceptar y continuar': 'Accept and continue',
    'Leer la política de privacidad': 'Read the privacy policy',
    'Descargar manualmente': 'Download manually',
    'Guardar cambios': 'Save changes',
    'Servicio PeerWay': 'PeerWay service',
    'Todo lo necesario está incluido. No hay URL ni servidor que configurar.': 'Everything needed is included. There is no URL or server to configure.',
    'La infraestructura la gestiona PeerWay de forma automática. Si cambiamos de red o de servidor, tus dominios y tu app siguen funcionando sin que tengas que tocar nada.': 'PeerWay manages the infrastructure automatically. If we change networks or servers, your domains and app keep working without you touching anything.',
    'Contactos de confianza': 'Trusted contacts',
    'Pueden enviarte archivos sin pedir permiso.': 'They can send you files without asking for permission.',
    'Agregar': 'Add',
    'No hay contactos de confianza.': 'No trusted contacts.',
    'Acerca de PeerWay': 'About PeerWay',
    'Archivos directos, cifrados y una nube opcional. Tu código y tus archivos te pertenecen.': 'Direct, encrypted files and an optional cloud. Your code and your files belong to you.',
    'Carpeta de datos': 'Data folder',
    'SOLICITUD ENTRANTE': 'INCOMING REQUEST',
    'Un equipo quiere enviarte archivos': 'A device wants to send you files',
    'Rechazar': 'Decline',
    'Aceptar y recibir': 'Accept and receive',
    'Conexión cifrada y verificada.': 'Encrypted, verified connection.',
    'Aceptar': 'Accept',
    'Cancelar': 'Cancel',
    'Equipo desconocido': 'Unknown device',
    'Buscando ruta…': 'Finding route…',
    'Canal cifrado': 'Encrypted channel',
    'Transferencia': 'Transfer',
    'TRANSFERENCIA': 'TRANSFER',
    'Enviar': 'Send',
    'Cuenta': 'Account',
    'sin nube': 'no cloud',
    'Gratis · sin nube': 'Free · no cloud',
    'Nube desactivada': 'Cloud disabled',
    'Completa usuario y contraseña.': 'Enter a username and password.',
    'Creando cuenta…': 'Creating account…',
    'Entrando…': 'Signing in…',
    'Cuenta creada. Tu código es permanente.': 'Account created. Your code is permanent.',
    'Sesión iniciada.': 'Signed in.',
    'Sesión cerrada.': 'Signed out.',
    'Escribe una licencia.': 'Enter a license.',
    'Plan Pro activado.': 'Pro plan activated.',
    'No hay una URL de pago configurada.': 'No payment URL is configured.',
    'PayPal no está disponible ahora. Puedes activar una licencia.': 'PayPal is not available right now. You can activate a license.',
    'No se pudo cargar PayPal. Revisa tu conexión o usa una licencia.': 'PayPal could not load. Check your connection or use a license.',
    'Pago completado. Plan Pro activado.': 'Payment completed. Pro plan activated.',
    'No se pudo completar el pago con PayPal.': 'PayPal payment could not be completed.',
    'Ajustes guardados.': 'Settings saved.',
    'Código inválido.': 'Invalid code.',
    'Contacto agregado como confiable.': 'Contact added as trusted.',
    'Transferencia cancelada.': 'Transfer canceled.',
    'Código copiado.': 'Code copied.',
    'Enlace copiado. Al abrirlo se muestra el código.': 'Link copied. Opening it shows the code.',
    'Recepción aceptada.': 'Receiving accepted.',
    'Solicitud rechazada.': 'Request declined.',
    'Copiando archivos al área local…': 'Copying files to the local area…',
    'Conectando': 'Connecting',
    'Pendiente': 'Pending',
    'Esperando aceptación': 'Waiting for acceptance',
    'Aceptada': 'Accepted',
    'Transfiriendo': 'Transferring',
    'Completado': 'Completed',
    'Falló': 'Failed',
    'Rechazado': 'Declined',
    'Cancelado': 'Canceled',
    'Un equipo': 'A device',
    'El código debe tener 8 caracteres.': 'The code must be 8 characters.',
    'Transferencia iniciada. Esperando aceptación.': 'Transfer started. Waiting for acceptance.',
    'Inicia sesión de nuevo para usar la nube.': 'Sign in again to use the cloud.',
    'Subiendo…': 'Uploading…',
    'No se pudo subir el archivo.': 'The file could not be uploaded.',
    'Error de red al subir el archivo.': 'Network error while uploading the file.',
    'Archivo borrado de la nube.': 'File deleted from the cloud.',
    'PeerWay ya está actualizado.': 'PeerWay is up to date.',
    'No se pudo buscar la actualización.': 'Could not check for updates.',
    'Instalando actualización…': 'Installing update…',
    'Actualización disponible.': 'Update available.',
    'Puedes instalarla ahora o desde la bandeja del sistema.': 'You can install it now or from the system tray.',
    'Instalar ahora': 'Install now',
    'Más tarde': 'Later',
    'Buscar actualizaciones': 'Check for updates',
    'La instalación se hace desde el menú de la bandeja del sistema.': 'Installation is started from the system tray menu.',
    'No se pudo instalar la actualización.': 'The update could not be installed.',
    'Idioma / Language': 'Language / Idioma',
    'error interno del servidor': 'internal server error',
    'falta la sesión': 'missing session',
    'sesión inválida o expirada': 'invalid or expired session',
    'ese usuario ya existe': 'that username already exists',
    'ese código no está conectado': 'that code is not connected',
    'registro inválido': 'invalid record',
    'anuncia tu código primero': 'announce your code first',
    'registro no vigente': 'record is no longer valid',
    'firma inválida': 'invalid signature',
    'túnel inválido': 'invalid tunnel',
    'archivo no encontrado': 'file not found',
    'no autorizado': 'not authorized',
    'cuenta no encontrada': 'account not found',
    'plan inválido': 'invalid plan',
    'orden no encontrada': 'order not found',
    'orden de otra cuenta': 'order belongs to another account',
    'PayPal no pudo crear la orden': 'PayPal could not create the order',
    'PayPal no pudo capturar la orden': 'PayPal could not capture the order',
    'demasiadas solicitudes; espera un momento': 'too many requests; please wait a moment',
    'El usuario debe tener entre 3 y 32 caracteres': 'The username must be between 3 and 32 characters',
    'Solo se permiten letras, números, punto, guion y guion bajo': 'Only letters, numbers, dot, hyphen and underscore are allowed',
    'La contraseña debe tener al menos 8 caracteres': 'The password must have at least 8 characters',
    'La contraseña es demasiado larga': 'The password is too long',
    'licencia inválida': 'invalid license',
    'licencia ya utilizada': 'license already used',
    'token local inválido': 'invalid local token',
    'no hay archivos válidos': 'no valid files',
    'transferencia no encontrada': 'transfer not found',
    'sin carpeta disponible': 'no folder available',
    'inicia sesión para usar la nube': 'sign in to use the cloud',
    'no se recibió ningún archivo': 'no file was received',
    'el nombre del equipo no puede estar vacío': 'the device name cannot be empty',
    'la carpeta de descargas no puede estar vacía': 'the downloads folder cannot be empty',
    'La cuenta se movió a otro dispositivo. Se generó un código temporal.': 'The account moved to another device. A temporary code was generated.',
    'Tu código temporal estaba ocupado. Se generó uno nuevo.': 'Your temporary code was taken. A new one was generated.',
    'Tu sesión expiró. Inicia sesión de nuevo.': 'Your session expired. Sign in again.',
    'Sesión iniciada. Tu código ya es permanente.': 'Signed in. Your code is now permanent.',
    'Sesión cerrada. Ahora tienes un código temporal.': 'Signed out. You now have a temporary code.',
    'Reinicia PeerWay para aplicar los cambios de red/servidor.': 'Restart PeerWay to apply the network/server changes.',
    'no puedes confiar en tu propio código': 'you cannot trust your own code',
    'no se pudo cerrar la sesión': 'could not sign out',
    'no se pudo leer el archivo': 'could not read the file',
    'no se pudo abrir el archivo': 'could not open the file',
    'el selector de archivos tardó demasiado': 'the file picker timed out',
    'código inválido': 'invalid code',
    'código de destino inválido': 'invalid destination code',
    'el registro del destino no es válido': 'the destination record is invalid',
    'el servidor devolvió un registro con otro código': 'the server returned a record with a different code',
    'no se encontró ese código. Verifica que PeerWay esté abierto en el otro equipo': 'that code was not found. Check that PeerWay is open on the other device',
    'la transferencia supera el límite de 4 TiB': 'the transfer exceeds the 4 TiB limit',
    'el receptor no respondió a tiempo': 'the receiver did not respond in time',
    'el receptor no envió el estado de reanudación': 'the receiver did not send the resume state',
    'el receptor no confirmó la recepción': 'the receiver did not confirm receipt',
    'confirmación inesperada del receptor': 'unexpected confirmation from the receiver',
    'tiempo de conexión agotado': 'connection timed out',
    'la identidad criptográfica del destino no coincide': 'the destination cryptographic identity does not match',
    'el relay no se emparejó a tiempo': 'the relay did not match in time',
    'la identidad criptográfica del destino no coincide (relay)': 'the destination cryptographic identity does not match (relay)',
    'el archivo cambió mientras se enviaba': 'the file changed while it was being sent',
    'el archivo se acortó durante el envío': 'the file shrank during the send',
    'el archivo cambió de tamaño durante el envío': 'the file changed size during the send',
    'no llegó una oferta': 'no offer arrived',
    'la oferta tiene una firma inválida': 'the offer has an invalid signature',
    'una oferta no puede venir de tu propio código': 'an offer cannot come from your own code',
    'el código no coincide con la identidad anunciada': 'the code does not match the announced identity',
    'id de transferencia inválido': 'invalid transfer id',
    'la oferta por relay no coincide con la señalización': 'the relay offer does not match the signaling',
    'metadatos de transferencia inválidos': 'invalid transfer metadata',
    'índice inválido': 'invalid index',
    'el emisor cambió los metadatos del archivo': 'the sender changed the file metadata',
    'file_end para un índice distinto': 'file_end for a different index',
    'mensaje inesperado durante la recepción': 'unexpected message during receive',
    'el relay cerró la conexión': 'the relay closed the connection',
    'el relay no emparejó la conexión': 'the relay did not match the connection',
    'conexión de rendezvous agotada': 'rendezvous connection exhausted',
    'mensaje de rendezvous inválido': 'invalid rendezvous message',
    'no llegó oferta por relay': 'no offer arrived through the relay',
    'esquema inválido': 'invalid scheme',
    'URL del servidor inválida': 'invalid server URL',
    'puerto UDP abierto automáticamente': 'UDP port opened automatically',
    'PRIMEROS PASOS': 'GETTING STARTED',
    'Guía rápida': 'Quick guide',
    'Comparte tu código': 'Share your code',
    'Compártelo con quien quiera enviarte archivos.': 'Share it with anyone who wants to send you files.',
    'Acepta o rechaza envíos': 'Accept or decline sends',
    'Cada solicitud espera tu respuesta antes de recibir nada.': 'Every request waits for your answer before anything is received.',
    'La nube es opcional': 'The cloud is optional',
    'Las transferencias van directas; la nube solo guarda archivos si la activas.': 'Transfers go direct; the cloud only stores files if you turn it on.',
    'Entendido': 'Got it',
    'Ver la guía': 'View the guide',
    'Cerrar la guía': 'Close the guide',
    'Sesiones': 'Sessions',
    'Equipos donde tu cuenta está abierta.': 'Devices where your account is open.',
    'Cerrar las demás sesiones': 'Sign out other sessions',
    'Cargando sesiones…': 'Loading sessions…',
    'No hay otras sesiones.': 'No other sessions.',
    'Inicia sesión para ver tus sesiones.': 'Sign in to see your sessions.',
    'Revocar': 'Revoke',
    'Sesión revocada.': 'Session revoked.',
    'Las demás sesiones se cerraron.': 'Other sessions were signed out.',
    '¿Cerrar las demás sesiones?': 'Sign out all other sessions?',
    'Dispositivo desconocido': 'Unknown device',
    'Tu correo aún no está confirmado. Pulsa «Enviar enlace» y revisa tu bandeja.': 'Your email is not confirmed yet. Press “Send link” and check your inbox.',
    'Quitar': 'Remove',
    'Descargar': 'Download',
    'Borrar': 'Delete',
    'Licencia': 'License',
    'Código de confianza': 'Trusted code',
    'Archivo para subir a la nube': 'File to upload to the cloud',
    'Comentario (opcional)': 'Comment (optional)',
    'Para qué son estos archivos': 'What these files are for',
    'Envíos recientes': 'Recent sends',
    'Pulsa uno para volver a enviar.': 'Tap one to send again.',
    'Todavía no enviaste archivos.': 'You have not sent any files yet.',
    'Crear enlace web': 'Create web link',
    'Contactos': 'Contacts',
    'Mostrar códigos': 'Show codes',
    'Válido 24 h y solo en tu red local. El archivo se sirve desde este equipo; no se sube a la nube.': 'Valid for 24 h and only on your local network. The file is served from this computer; it is not uploaded to the cloud.',
    'Copiar': 'Copy',
    '3–32 caracteres, sin espacios ni acentos: letras, números, punto, guion y guion bajo.': '3–32 characters, no spaces or accents: letters, numbers, dot, dash, underscore.',
    '••••-••••': '••••-••••',
  };

  const RULES = [
    [/^(\d+) en la red$/, (m) => `${m[1]} on the network`],
    [/^(\d+) archivos?$/, (m) => `${m[1]} file${m[1] === '1' ? '' : 's'}`],
    [/^(\d+) archivos? · (.+)$/, (m) => `${m[1]} file${m[1] === '1' ? '' : 's'} · ${m[2]}`],
    [/^(\d+) transferencias?$/, (m) => `${m[1]} transfer${m[1] === '1' ? '' : 's'}`],
    [/^(\d+) equipo\(s\) en tu LAN$/, (m) => `${m[1]} device${m[1] === '1' ? '' : 's'} on your LAN`],
    [/^(.+) · (\d+) archivo(?:\(s\)|s?)$/, (m) => `${m[1]} · ${m[2]} file${m[2] === '1' ? '' : 's'}`],
    [/^(.+) quiere enviarte archivos$/, (m) => `${translateCapture(m[1])} wants to send you files`],
    [/^Actualización (.+) disponible en la bandeja\.$/, (m) => `Update ${m[1]} available in the tray.`],
    [/^Actualización (.+) disponible\.$/, (m) => `Update ${m[1]} available.`],
    [/^Subiendo (\d+)%$/, (m) => `Uploading ${m[1]}%`],
    [/^¿Borrar «(.+)» de la nube\?$/, (m) => `Delete «${m[1]}» from the cloud?`],
    [/^«(.+)» guardado en la nube\.$/, (m) => `«${m[1]}» saved to the cloud.`],
    [/^\+(\d+) más$/, (m) => `+${m[1]} more`],
    [/^No se pudo leer el estado: (.+)$/, (m) => `Could not read status: ${m[1]}`],
    [/^(\d+) B de (.+)$/, (m) => `${m[1]} B of ${m[2]}`],
    [/^Aceptando automáticamente archivos de (.+) \(contacto de confianza\)\.$/, (m) => `Automatically accepting files from ${m[1]} (trusted contact).`],
    [/^(.+) de nube$/, (m) => `${m[1]} of cloud`],
    [/^(.+) de (.+)$/, (m) => `${m[1]} of ${m[2]}`],
    [/^(.+) · código (.+)$/, (m) => `${translateCapture(m[1])} · code ${m[2]}`],
    [/^Para (.+)$/, (m) => `To ${m[1]}`],
    [/^De (.+)$/, (m) => `From ${m[1]}`],
    [/^sin conexión con el servidor: (.+)$/, (m) => `not connected to the server: ${m[1]}`],
    [/^respuesta inválida: (.+)$/, (m) => `invalid response: ${m[1]}`],
    [/^señalización: (.+)$/, (m) => `signaling: ${m[1]}`],
    [/^el servidor no tiene dirección para (IPv\d+)$/, (m) => `the server has no address for ${m[1]}`],
    [/^no se pudo establecer una conexión con (.+)\. Revisa que el receptor esté conectado y que el firewall permita UDP$/, (m) => `could not connect to ${m[1]}. Check that the receiver is online and the firewall allows UDP`],
    [/^la verificación SHA-256 del archivo (.+) falló$/, (m) => `SHA-256 verification failed for file ${m[1]}`],
  ];

  if (typeof module === 'object' && module.exports) {
    module.exports = { DICT, RULES };
    return;
  }

  const STORAGE_KEY = 'peerway.lang';
  const stored = localStorage.getItem(STORAGE_KEY);
  const browserLanguage = (navigator.language || 'es').toLowerCase();
  let lang = stored === 'en' || stored === 'es'
    ? stored
    : (browserLanguage.startsWith('es') ? 'es' : 'en');
  let applying = false;
  const textOriginals = new WeakMap();
  const attributeOriginals = new WeakMap();
  const ATTRIBUTES = ['placeholder', 'title', 'aria-label'];
  let observer = null;

  function translateCapture(value) {
    return DICT[value] || value;
  }

  function lookup(text) {
    if (lang === 'es') return text;
    if (DICT[text]) return DICT[text];
    for (const [pattern, replacer] of RULES) {
      const match = text.match(pattern);
      if (match) return replacer(match);
    }
    return text;
  }

  function preserveWhitespace(value) {
    const match = String(value).match(/^(\s*)([\s\S]*?)(\s*)$/);
    if (!match) return value;
    return `${match[1]}${lookup(match[2])}${match[3]}`;
  }

  function skippable(element) {
    return !element || !!element.closest('script, style, option, textarea');
  }

  function translateTextNode(node) {
    const parent = node.parentElement;
    if (!parent || skippable(parent)) return;
    const value = node.nodeValue;
    if (!value || !value.trim()) return;
    const original = textOriginals.get(node);
    if (original !== undefined && value === preserveWhitespace(original)) return;
    textOriginals.set(node, value);
    const translated = preserveWhitespace(value);
    if (translated !== value) {
      applying = true;
      node.nodeValue = translated;
      applying = false;
    }
  }

  function translateAttribute(element, name) {
    if (!ATTRIBUTES.includes(name) || skippable(element)) return;
    const value = element.getAttribute(name);
    if (!value) return;
    const originals = attributeOriginals.get(element) || {};
    attributeOriginals.set(element, originals);
    const original = originals[name];
    if (original !== undefined && value === preserveWhitespace(original)) return;
    originals[name] = value;
    const translated = preserveWhitespace(value);
    if (translated !== value) {
      applying = true;
      element.setAttribute(name, translated);
      applying = false;
    }
  }

  function translateTree(root) {
    if (!root) return;
    if (root.nodeType === Node.TEXT_NODE) {
      translateTextNode(root);
      return;
    }
    if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE) return;
    if (root.nodeType === Node.ELEMENT_NODE) {
      ATTRIBUTES.forEach((name) => {
        if (root.hasAttribute(name)) translateAttribute(root, name);
      });
    }
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    let current = walker.nextNode();
    while (current) {
      if (current.nodeType === Node.TEXT_NODE) {
        translateTextNode(current);
      } else {
        ATTRIBUTES.forEach((name) => {
          if (current.hasAttribute(name)) translateAttribute(current, name);
        });
      }
      current = walker.nextNode();
    }
  }

  function restoreAll() {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    let current = walker.nextNode();
    while (current) {
      if (current.nodeType === Node.TEXT_NODE) {
        const original = textOriginals.get(current);
        if (original !== undefined && current.nodeValue !== original) current.nodeValue = original;
      } else {
        const originals = attributeOriginals.get(current);
        if (originals) {
          for (const [name, original] of Object.entries(originals)) {
            if (current.getAttribute(name) !== original) current.setAttribute(name, original);
          }
        }
      }
      current = walker.nextNode();
    }
  }

  function refresh() {
    const select = document.getElementById('setLanguage');
    if (select) select.value = lang;
    document.documentElement.lang = lang;
  }

  function setLanguage(next) {
    if (next !== 'en' && next !== 'es') return;
    lang = next;
    localStorage.setItem(STORAGE_KEY, lang);
    restoreAll();
    translateTree(document.body);
    refresh();
    document.dispatchEvent(new CustomEvent('peerway-language', { detail: { lang } }));
  }

  function t(value) {
    return lookup(String(value));
  }

  function locale() {
    return lang === 'es' ? 'es' : 'en';
  }

  function start() {
    document.documentElement.lang = lang;
    translateTree(document.body);
    refresh();
    observer = new MutationObserver((mutations) => {
      if (applying) return;
      for (const mutation of mutations) {
        if (mutation.type === 'characterData') {
          translateTextNode(mutation.target);
        } else if (mutation.type === 'childList') {
          mutation.addedNodes.forEach((node) => translateTree(node));
        } else if (mutation.type === 'attributes') {
          translateAttribute(mutation.target, mutation.attributeName);
        }
      }
    });
    observer.observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: ATTRIBUTES,
    });
    document.addEventListener('change', (event) => {
      if (event.target?.id === 'setLanguage') setLanguage(event.target.value);
    });
    const select = document.getElementById('setLanguage');
    if (select) select.value = lang;
  }

  window.PeerWayI18n = { t, locale, setLanguage, get lang() { return lang; } };
  start();
})();
