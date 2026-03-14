# Plan: Chrome Extension for Bookmark Capture (flt-it8.9)

## Context

Flotsam needs a Chrome extension for one-click bookmark capture. The API already has a `captureBookmark` GraphQL mutation that accepts URL, title, selected text, tags, and sensitivity level. The extension authenticates via JWT Bearer token.

## API Interface

**Endpoint:** `POST /graphql`
**Auth:** `Authorization: Bearer <JWT_TOKEN>`
**Mutation:**
```graphql
mutation CaptureBookmark($input: BookmarkInput!) {
  captureBookmark(input: $input) { id status title tags }
}
```
**BookmarkInput fields:** `url` (required), `title`, `selection`, `tags: [String!]`, `sensitivity: SensitivityLevel`

## Structure

```
chrome-extension/
  manifest.json          # Manifest V3
  popup/
    popup.html           # Main UI
    popup.css            # Styles
    popup.js             # Popup logic
  background/
    service-worker.js    # API communication
  content/
    content.js           # Grab selected text from page
  options/
    options.html         # Settings page
    options.css
    options.js           # Save API URL + JWT token
  icons/
    icon16.png           # Placeholder icons (generated as simple colored squares)
    icon48.png
    icon128.png
  README.md              # Dev setup instructions
```

## Implementation Details

### manifest.json
- Manifest V3, `action` with popup, `background.service_worker`
- Permissions: `activeTab`, `storage`, `scripting`
- Host permissions: `<all_urls>` (needed to inject content script on any page)
- Content script registered on-demand via `chrome.scripting.executeScript` (no persistent content script injection)

### popup (popup/)
- Shows current tab URL (pre-filled, read-only)
- Page title (pre-filled from tab)
- Selected text area (auto-filled from content script, editable)
- Tags input (comma-separated)
- Sensitivity dropdown (Public/Normal/Sensitive/Private, default Normal)
- Save button → sends message to service worker
- Status feedback (success/error)
- Clean minimal UI with system fonts, no framework

### background/service-worker.js
- Listens for messages from popup
- Makes GraphQL fetch to configured API URL
- Attaches JWT Bearer token from storage
- Returns success/error to popup

### content/content.js
- Executed on-demand by popup via `chrome.scripting.executeScript`
- Returns `window.getSelection().toString()`

### options (options/)
- Two fields: API URL, JWT Token
- Save to `chrome.storage.local`
- Validate inputs before saving

### icons/
- Generate simple placeholder PNGs programmatically (solid color squares) using a canvas-based Node script, or just provide minimal valid PNG files

## Verification

1. `manifest.json` is valid JSON and follows Manifest V3 spec
2. All JS files parse without syntax errors (verify with `node --check`)
3. README documents loading as unpacked extension
4. No external dependencies — pure vanilla JS/HTML/CSS
