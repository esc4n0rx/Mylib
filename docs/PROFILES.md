# Profiles and parental controls

Authentication continues to use a user account. Media consumption uses the profile identifier
embedded in the signed access token returned by `POST /api/v1/profiles/{id}/select`. A token
without a profile may only be used to list and select profiles; media endpoints require a selected
profile.

Effective media visibility is the intersection of user library access, profile library access,
the library minimum age, the profile maximum age, and the normalized content rating. Unrated
content is blocked for kids profiles by default. Administrators can change that policy through
`/api/v1/parental-controls/settings`.

Avatar images live under `<MYLIB_DATA_DIR>/avatars` in the fixed categories `dp`, `nf`, `pop`,
`pp`, and `pv`. The server creates these directories, indexes safe image filenames lazily, returns
only paginated metadata, and serves image bytes separately with HTTP caching. Missing saved images
use the built-in fallback; images and avatar URLs are never stored in the database.

PINs contain four to six digits, are stored only as Argon2id hashes, and are limited to five failed
unlock attempts per profile per minute. An unlocked profile remains selected until another profile
is selected or the user signs out.
