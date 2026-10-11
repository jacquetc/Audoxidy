# Audoxidy (Teksilo UI): The library panel (view modes, column header, rows, bottom bar).
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Delete (quirk fix 1)
# Not in the Iced UI: the library deletes without confirmation (quirk fix 1).
library-delete-title = ¿Eliminar de la biblioteca?
# Not in the Iced UI: the confirmation of quirk fix 1, worded as the Iced playlist deletion.
library-delete-text =
    ¿Estás seguro de querer eliminar
    { $count ->
        [one] { $count_text } canción
       *[other] { $count_text } canciones
    } de la biblioteca?
    Esta acción no se puede deshacer.
library-delete-confirm = Eliminar
# Not in the Iced UI: error toast of quirk fix 8.
library-delete-failed = No se pudieron eliminar las canciones

## Add a folder
# Not in the Iced UI: error toast of quirk fix 8.
library-scan-failed = No se pudo escanear la carpeta

## Header rows
library-artist-stats =
    { $songs ->
        [one] { $songs_text } Canción
       *[other] { $songs_text } Canciones
    } | { $albums ->
        [one] { $albums_text } Álbum
       *[other] { $albums_text } Álbumes
    } | { $duration }
library-album-stats =
    { $songs ->
        [one] { $songs_text } Canción
       *[other] { $songs_text } Canciones
    } | { $duration }
# Not in the Iced UI: accessible name of the chevron of an open group.
library-collapse = Plegar { $name }
# Not in the Iced UI: accessible name of the chevron of a closed group.
library-expand = Desplegar { $name }

## Panel
library-panel = Biblioteca
# Not in the Iced UI: accessible name of the album grid.
library-grid = Álbumes
# Not in the Iced UI: accessible name of the detail panel under the grid.
library-album-songs = Canciones del álbum
# Not in the Iced UI: accessible name of the song list.
library-list = Canciones
# Not in the Iced UI: accessible name of a card's chevron.
library-open-album = Abrir o cerrar { $album }
library-empty = La biblioteca está vacía.
library-no-albums = No se encontraron álbumes.
library-empty-or-loading = La biblioteca está vacía o cargando...

## Bottom bar
library-search-placeholder = Buscar...
# Not in the Iced UI: accessible name of the search field.
library-search = Buscar en la biblioteca
library-stats =
    { $songs ->
        [one] { $songs_text } Canción
       *[other] { $songs_text } Canciones
    } | { $albums ->
        [one] { $albums_text } Álbum
       *[other] { $albums_text } Álbumes
    } | { $artists ->
        [one] { $artists_text } Artista
       *[other] { $artists_text } Artistas
    } | { $duration } | { $size }
# Not in the Iced UI: accessible name of the play icon.
library-play = Reproducir la selección
# Not in the Iced UI: accessible name of the "more" icon, which adds a folder.
library-add-folder = Añadir una carpeta
# Not in the Iced UI: accessible name of the radio icon, which shows what plays.
library-show-playing = Mostrar lo que suena
# Not in the Iced UI: accessible name of the view-mode icon.
library-view-mode = Cambiar de vista
