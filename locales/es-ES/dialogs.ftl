# Audoxidy (Teksilo UI): Dialogs shared by several panels (playlist create, rename, delete, export).
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## Create playlist
dialogs-create-default-name = Nueva lista { $number }
dialogs-create-title = Nueva lista de reproducción
dialogs-create-prompt = Nombre para la nueva lista:
dialogs-create-placeholder = Nombre de la lista...
dialogs-create-confirm = Crear

## Rename playlist
dialogs-rename-title = Renombrar lista
dialogs-rename-prompt = Nuevo nombre de la lista:
dialogs-rename-placeholder = Nuevo nombre...
dialogs-rename-confirm = Renombrar

## Delete playlist
dialogs-delete-title = ¿Eliminar lista?
dialogs-delete-text =
    ¿Estás seguro de querer eliminar
    la lista "{ $name }"?
    Esta acción no se puede deshacer.
dialogs-delete-confirm = Eliminar

## Save or export a playlist
dialogs-export-title = Guardar | Exportar lista
dialogs-export-intro =
    Puedes guardar solo la lista o también
    exportar todas las canciones de la lista
    "{ $name }".
dialogs-export-format = Formato de la lista:
dialogs-export-mode = Modo:
dialogs-export-single = Solo Lista
dialogs-export-portable = Carpeta Portable
dialogs-export-confirm = Continuar

## File dialogs
dialogs-export-folder-title = Seleccionar carpeta de exportación
dialogs-export-save-title = Guardar lista de reproducción
dialogs-playlist-filter = Lista de reproducción
dialogs-audio-filter = Audio
dialogs-lists-filter = Listas
