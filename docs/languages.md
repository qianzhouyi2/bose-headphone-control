# Language packs

The app includes English and Simplified Chinese. It follows the Windows display language by default; users can select a language under **About headphones → Language**.

To add a translation, create a `Languages` directory beside the executable and place a UTF-8 JSON file inside it:

```json
{
  "code": "fr",
  "nativeName": "Français",
  "strings": {
    "nav.home": "Contrôles",
    "action.resetEq": "Réinitialiser"
  }
}
```

Restart the app to load the pack. Each language code must be unique. Missing translations fall back to English. Use [languages/en.json](../languages/en.json) for the complete set of keys, and preserve placeholders such as `{0}`.

Device names and custom listening mode names are displayed as reported by the headphones.

For a bundled translation, add the catalog under `languages/` and register it in `Localization::new`. External packs use `Localization::register`; `set` saves the preference and `text` retrieves a translation.
