# Audoxidy (Teksilo UI): The playlist panel (tabs, rows, menus, bottom bar).
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Toasts
# Not in the Iced UI: the export succeeded (Iced says nothing).
playlist-toast-exported = Lista exportada
# Not in the Iced UI: the export wrote its list but some copies failed.
playlist-toast-exported-partly = Lista exportada con errores
# Not in the Iced UI: how many copies of a portable export failed.
playlist-toast-copies-failed =
    { $count ->
        [one] No se pudo copiar { $count } canción
       *[other] No se pudieron copiar { $count } canciones
    }
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-export-failed = No se pudo exportar la lista
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-import-failed = No se pudo importar la lista
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-add-failed = No se pudieron agregar los archivos
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-create-failed = No se pudo crear la lista
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-rename-failed = No se pudo renombrar la lista
# Not in the Iced UI: quirk fix 8, a failure Iced only logs.
playlist-toast-delete-failed = No se pudo eliminar la lista
# Not in the Iced UI: quirk fix 8, a failure Iced ignores.
playlist-toast-location-failed = No se pudo abrir la ubicación del archivo
