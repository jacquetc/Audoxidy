# Audoxidy (Teksilo UI): The player panel (cover, transport, seek bar, volume overlay, player menu).
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Player menu
# Not in the Iced UI: accessible name of the menu button, which is an icon.
player-menu-button = Menú
# Not in the Iced UI: accessible name of the menu.
player-menu-label = Menú del reproductor
player-menu-open-file = Abrir Archivo
player-menu-open-folder = Abrir Carpeta
player-menu-library = Biblioteca
# Iced: "Lista de Reproduccion" (fix 15).
player-menu-playlist = Lista de Reproducción
# Iced: "Lirycs" (fix 15).
player-menu-lyrics = Lyrics
player-menu-audio-settings = Configuración de Audio
player-menu-equalizer = Ecualizador
player-menu-effects = Efectos de Audio
# Iced: "Apagado Automatico" (fix 15).
player-menu-auto-shutdown = Apagado Automático
# Iced: "Personalizacion" (fix 15).
player-menu-customization = Personalización
player-menu-preferences = Preferencias
player-menu-plugins = Complementos
player-menu-about = Acerca
player-menu-quit = Salir

## Controls
# Not in the Iced UI: accessible name of the player.
player-name = Reproductor
# Not in the Iced UI: accessible names of the three control zones, which are drawn as icons.
player-previous = Pista anterior
# Not in the Iced UI: the Iced UI draws this control as an icon with no text; the name is for assistive technology.
player-play-pause = Reproducir o pausar
# Not in the Iced UI: the Iced UI draws this control as an icon with no text; the name is for assistive technology.
player-next = Pista siguiente
# Not in the Iced UI: accessible name of the seek slider.
player-seek = Posición de reproducción
# Not in the Iced UI: the seek slider's position as assistive technology reads it.
player-seek-value = { $elapsed } de { $total }

## Errors
# Not in the Iced UI: the Iced UI only logs a failed scan.
player-scan-failed = No se pudo escanear la carpeta: { $error }
# Not in the Iced UI: the Iced UI has no error for the folder dialog.
player-folder-dialog-failed = Falló el diálogo de carpetas: { $error }
# Not in the Iced UI: the Iced UI only logs a failed track change.
player-track-failed = No se pudo reproducir la canción: { $error }
