# Implementing an additional server version of Cantara

With this spec, Cantara should be enhenced with a fullstack option that allows the app to be hosted as a webserver. Clients can than connect to it (via Cantara Sync) and use the server's repository items, design settings, slide settings and more.
In addition to that, people can use the web ui of the server to directly start and configure presentations or to explore the repositories (all features of desktop version should be available). Running presentations can also be shared via link (presentation and controle) or with other users.

The following things are needed:
- Authentication, tenants (organizations) and groups: All data is tenant-seperate, users and groups are needed for permission
- PostgreSQL database backend for repository items, settings etc.: A Postgres is intended to save repository items such as files, settings, etc. Every item should have `uuid` (a uuid v7) as primary key. Files should be versionable with each version getting a new uuid. The uuid of songs (tag uuid) will be saved in an additional field `lineage_uuid` to track changes. Where such a uuid doesn't exist yet, a new one wil be assigned.
- Admins (a special group) will be able to configure users, groups and permissions. Group managers can invite other people into their group.
- Item selections can be saved and managed as well as shared with other people and groups (e.g. for church service preparations)
- A running presentation can instantly be shared via links, users and groups.
- OpenID-support


