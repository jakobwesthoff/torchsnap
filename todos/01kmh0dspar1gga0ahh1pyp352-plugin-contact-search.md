# Plugin: Contact search and display

Search system contacts and display contact details in the launcher.

## Scope

- Index contacts from the system address book
- Fuzzy search by name, email, phone, company
- Display contact card with photo, name, key details
- Actions: copy email, copy phone, compose email, call (if
  supported)

## Platform considerations

- **macOS**: Contacts.framework (requires permission prompt)
- **Linux**: Evolution Data Server, or CardDAV
- **Windows**: Windows.ApplicationModel.Contacts API

## Privacy

- Requires explicit user permission to access contacts
- Contact data should never leave the local machine
- Index only — no copying of contact data to plugin storage
