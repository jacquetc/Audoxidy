# Audoxidy (Teksilo UI): Audio Center: the equalizer tab and its preset dialogs.
# Spanish: the Iced UI's strings, verbatim except the corrections listed in
# docs/teksilo-port-quirk-fixes.md (fix 15). A message the Iced UI does not
# have is marked `# Not in the Iced UI: <why>.`

## The tab
ac-equalizer-toggle = Activar Ecualizador
ac-equalizer-bands = Bandas:
ac-equalizer-preset-label = Preset: { $name }
# Not in the Iced UI: accessible name of an icon button.
ac-equalizer-load-button = Cargar preset
# Not in the Iced UI: accessible name of an icon button.
ac-equalizer-save-button = Guardar preset
# Not in the Iced UI: accessible name of an icon button.
ac-equalizer-reset-button = Restablecer ecualizador
# Not in the Iced UI: the slider shows "Pre", which is no name.
ac-equalizer-preamp-name = Preamplificación
# Not in the Iced UI: the slider shows its frequency only.
ac-equalizer-band-name = Banda de { $frequency } Hz
ac-equalizer-hint = *Puede restablecer al valor por defecto haciendo clic secundario sobre un deslizador.

## The Load dialog
ac-equalizer-load-title = Cargar Preset
# Not in the Iced UI: accessible name of the preset list.
ac-equalizer-load-list = Presets
ac-equalizer-load-accept = Aceptar
ac-equalizer-load-delete = Eliminar
ac-equalizer-load-import = Importar
ac-equalizer-load-export = Exportar
ac-equalizer-load-defaults = Predeterminado
# Iced: "se elimarán" (fix 15).
ac-equalizer-load-warning = Advertencia: Se eliminarán los presets personalizados y se restablecerán los presets predeterminados.
ac-equalizer-import-title = Importar preset de ecualizador
ac-equalizer-export-title = Exportar preset de ecualizador

## The Save dialog
ac-equalizer-save-title = Guardar Preset
ac-equalizer-save-prompt = Nombre del preset:
ac-equalizer-save-placeholder = Nombre del preset...
ac-equalizer-save-confirm = Guardar

## Failures (each is an error toast as well as a log line)
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-save-failed = No se pudo guardar el preset: { $error }
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-delete-failed = No se pudo eliminar el preset: { $error }
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-defaults-failed = No se pudieron restablecer los presets predeterminados: { $error }
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-import-failed = No se pudo importar el preset: { $error }
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-export-failed = No se pudo exportar el preset: { $error }
# Not in the Iced UI: it only logs these failures (quirk fix 8).
ac-equalizer-dialog-failed = Falló el diálogo de archivos: { $error }
