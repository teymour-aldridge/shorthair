# todo: what is the correct restore behaviour
litestream restore -if-db-not-exists -if-replica-exists /data/sqlite.db \
&& litestream replicate -exec "sh /app/entrypoint.sh"
