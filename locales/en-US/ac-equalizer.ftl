# Audoxidy (Teksilo UI): Audio Center: the equalizer tab and its preset dialogs.
# Source locale: `tr!` validates keys against these files.

## The tab
ac-equalizer-toggle = Enable Equalizer
ac-equalizer-bands = Bands:
ac-equalizer-preset-label = Preset: { $name }
ac-equalizer-load-button = Load preset
ac-equalizer-save-button = Save preset
ac-equalizer-reset-button = Reset equalizer
ac-equalizer-preamp-name = Preamp
ac-equalizer-band-name = { $frequency } Hz band
ac-equalizer-hint = *You can restore the default value by right-clicking a slider.

## The Load dialog
ac-equalizer-load-title = Load Preset
ac-equalizer-load-list = Presets
ac-equalizer-load-accept = Accept
ac-equalizer-load-delete = Delete
ac-equalizer-load-import = Import
ac-equalizer-load-export = Export
ac-equalizer-load-defaults = Defaults
ac-equalizer-load-warning = Warning: the custom presets will be deleted and the default presets will be restored.
ac-equalizer-import-title = Import equalizer preset
ac-equalizer-export-title = Export equalizer preset

## The Save dialog
ac-equalizer-save-title = Save Preset
ac-equalizer-save-prompt = Preset name:
ac-equalizer-save-placeholder = Preset name...
ac-equalizer-save-confirm = Save

## Failures (each is an error toast as well as a log line)
ac-equalizer-save-failed = The preset could not be saved: { $error }
ac-equalizer-delete-failed = The preset could not be deleted: { $error }
ac-equalizer-defaults-failed = The default presets could not be restored: { $error }
ac-equalizer-import-failed = The preset could not be imported: { $error }
ac-equalizer-export-failed = The preset could not be exported: { $error }
ac-equalizer-dialog-failed = The file dialog failed: { $error }
