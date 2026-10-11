# Audoxidy (Teksilo UI): The library panel (view modes, column header, rows, bottom bar).
# Source locale: `tr!` validates keys against these files.

## Delete (quirk fix 1)
library-delete-title = Delete from the library?
library-delete-text =
    Are you sure you want to delete
    { $count ->
        [one] { $count_text } song
       *[other] { $count_text } songs
    } from the library?
    This action cannot be undone.
library-delete-confirm = Delete
library-delete-failed = The songs could not be deleted

## Add a folder
library-scan-failed = The folder could not be scanned

## Header rows
library-artist-stats =
    { $songs ->
        [one] { $songs_text } Song
       *[other] { $songs_text } Songs
    } | { $albums ->
        [one] { $albums_text } Album
       *[other] { $albums_text } Albums
    } | { $duration }
library-album-stats =
    { $songs ->
        [one] { $songs_text } Song
       *[other] { $songs_text } Songs
    } | { $duration }
library-collapse = Collapse { $name }
library-expand = Expand { $name }

## Panel
library-panel = Library
library-grid = Albums
# The detail panel under the grid.
library-album-songs = Album songs
library-list = Songs
library-open-album = Open or close { $album }
library-empty = The library is empty.
library-no-albums = No albums found.
library-empty-or-loading = The library is empty or loading...

## Bottom bar
library-search-placeholder = Search...
library-search = Search the library
library-stats =
    { $songs ->
        [one] { $songs_text } Song
       *[other] { $songs_text } Songs
    } | { $albums ->
        [one] { $albums_text } Album
       *[other] { $albums_text } Albums
    } | { $artists ->
        [one] { $artists_text } Artist
       *[other] { $artists_text } Artists
    } | { $duration } | { $size }
library-play = Play the selection
library-add-folder = Add a folder
library-show-playing = Show what is playing
library-view-mode = Change the view
