# Work item details

Click a work-item title to open its right-side detail dialog. The ID remains a
canonical external link, and the detail header offers the same source-page link
and local-mark editor. Escape or the close control dismisses the dialog without
changing the list's search, filters, scroll content or pagination.

Selection uses source, scope, address and native ID. It resolves against the
current snapshot, so refreshed versions and local edits appear without copying
stale item state into the dialog. Switching models/kinds or removing the selected
item closes the selection. The existing annotation editor returns to details when
opened there; row-level annotation entry continues to return to the list.

The body shows full version names, source and meaningful dates, progress, the
latest comment, tags, correction reasons, next-check date and complete local notes.
Meegle history opens on demand through the shared flow cache with full comment
text initially visible. Local observed history is a separate section and does not
repeat the snapshot's latest comment. Source HTML is converted through the same
readable-text utility used by flow rows; links remain clickable.

SPM now retains the complete latest Jira comment and Meegle progress. Stored Jira
rows no longer cut comment bodies to 500 characters; the source projection
fingerprint forces their next refresh to rebuild those rows. Existing report
snapshots remain visible but have an expired read timestamp until the version-2
full-text projection is written. Remote data stays read-only.

Detail portals are outside the image-export target. Existing title text remains
in list exports, with no extra row action controls added.
