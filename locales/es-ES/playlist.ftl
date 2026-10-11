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

## Menus
playlist-menu-play = Reproducir
playlist-menu-new-list = Nueva lista
playlist-menu-rename = Renombrar lista
playlist-menu-delete-list = Eliminar lista
playlist-menu-save = Guardar lista
playlist-menu-import = Importar lista
playlist-menu-export = Exportar lista
playlist-menu-add-files = Agregar archivo
playlist-menu-add-folder = Agregar carpeta
playlist-menu-info = Información
playlist-menu-file-location = Ubicación del archivo
playlist-menu-show-in-library = Mostrar en la biblioteca
playlist-menu-send-to = Enviar a otra lista
playlist-menu-tag-editor = Editor de Etiquetas
playlist-menu-converter = Convertidor de audio
playlist-menu-toggle-enabled = Activar | Desactivar
playlist-menu-fold = Plegar | Desplegar
playlist-menu-remove = Eliminar de la lista

## List
# Not in the Iced UI: the list's accessible name (as in the player menu's "Lista de Reproducción").
playlist-list = Lista de reproducción
playlist-empty = No hay canciones en esta lista
# Not in the Iced UI: read after the name of the playing song's row.
playlist-row-playing = Reproduciendo
# Not in the Iced UI: read after the name of a disabled song's row.
playlist-row-disabled = Desactivada

## Tab strip
# Not in the Iced UI: the tab strip's accessible name.
playlist-tabs = Listas de reproducción
# Not in the Iced UI: the overflow chevron's accessible name.
playlist-all-lists = Todas las listas

## Bottom bar
playlist-search = Buscar...
# Not in the Iced UI: the lyrics icon's accessible name (the player menu's "Lyrics", quirk fix 15).
playlist-lyrics = Lyrics
# Not in the Iced UI: the shuffle icon's accessible name.
playlist-shuffle = Aleatorio
# Not in the Iced UI: the repeat icon's accessible name, mode off.
playlist-repeat-off = Repetir: desactivado
# Not in the Iced UI: the repeat icon's accessible name, mode all.
playlist-repeat-all = Repetir: todas
# Not in the Iced UI: the repeat icon's accessible name, mode one.
playlist-repeat-one = Repetir: una
# Not in the Iced UI: the equalizer icon's accessible name (the player menu's "Ecualizador").
playlist-equalizer = Ecualizador
