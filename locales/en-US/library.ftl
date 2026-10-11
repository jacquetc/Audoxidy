# Audoxidy (Teksilo UI): The library panel (view modes, column header, rows, bottom bar).
# Source locale: `tr!` validates keys against these files.

## Delete (quirk fix 1)
library-delete-title = Delete from the library?
library-delete-text =
    Are you sure you want to delete
    { $count ->
        [one] { $count } song
       *[other] { $count } songs
    } from the library?
    This action cannot be undone.
library-delete-confirm = Delete
library-delete-failed = The songs could not be deleted

## Add a folder
library-scan-failed = The folder could not be scanned
