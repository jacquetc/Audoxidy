# Audoxidy (Teksilo UI): Dialogs shared by several panels (playlist create, rename, delete, export).
# Source locale: `tr!` validates keys against these files.

## Create playlist
# The name a new playlist gets when its name is left blank.
dialogs-create-default-name = New playlist { $number }
dialogs-create-title = New playlist
dialogs-create-prompt = Name for the new playlist:
dialogs-create-placeholder = Playlist name...
dialogs-create-confirm = Create

## Rename playlist
dialogs-rename-title = Rename playlist
dialogs-rename-prompt = New name for the playlist:
dialogs-rename-placeholder = New name...
dialogs-rename-confirm = Rename

## Delete playlist
dialogs-delete-title = Delete playlist?
dialogs-delete-text =
    Are you sure you want to delete
    the playlist "{ $name }"?
    This cannot be undone.
dialogs-delete-confirm = Delete

## Save or export a playlist
dialogs-export-title = Save | Export playlist
dialogs-export-intro =
    You can save only the list or also
    export every song of the playlist
    "{ $name }".
dialogs-export-format = List format:
dialogs-export-mode = Mode:
dialogs-export-single = List only
dialogs-export-portable = Portable folder
dialogs-export-confirm = Continue

## File dialogs
dialogs-export-folder-title = Choose the export folder
dialogs-export-save-title = Save playlist
# The file-type filter of the playlist file dialogs.
dialogs-playlist-filter = Playlist
dialogs-audio-filter = Audio
dialogs-lists-filter = Playlists
