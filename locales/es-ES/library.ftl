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
        [one] { $count } canción
       *[other] { $count } canciones
    } de la biblioteca?
    Esta acción no se puede deshacer.
library-delete-confirm = Eliminar
# Not in the Iced UI: error toast of quirk fix 8.
library-delete-failed = No se pudieron eliminar las canciones

## Add a folder
# Not in the Iced UI: error toast of quirk fix 8.
library-scan-failed = No se pudo escanear la carpeta
