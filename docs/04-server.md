# Server

The JSON server is written in Rust. It is using `axum` to create endpoints. It requires an access to the PostgreSQL database where the blocks and transactions have been stored.

## Configuration

Settings.toml

```toml
log_level = "info"
log_format = "pretty" # optional; either "pretty" or "json"
chain_name = "public-testnet-15" # IMPORANT! Do not use `.` the chain name is juste the Chain ID without what is after the '.' (e.g 'shielded-expedition.b40d8e9055' becomes 'shielded-expedition')

# Connection information for the PostgreSQL database
[database]
host = "localhost"
user = "postgres"
password = "wow"
dbname = "blockchain"
port = 5432
connection_timeout = 20 # Optional timeout value
create_index = true

[server]
serve_at = "0.0.0.0"
port = 30303
```

## Block Endpoints

The list of endpoints available.

### /block/height/:block_height

This enpoint look for a specific block by its `height`.

Example:

```
$ curl -H 'Content-Type: application/json' localhost:30303/block/height/1
```

### /block/hash/:block_hash

This endpoint look for a specific block by its `hash`.
Example:

```
$ curl -H 'Content-Type: application/json' localhost:30303/block/hash/9d6dad4409536ab763c0b814379be71ad1f9176efe17292f143831fbad72109c
```

### /block/last

This endpoint will return the last block indexed.
Example:

```
$ curl -H 'Content-Type: application/json' localhost:30303/block/last
```

## Transaction Endpoints

### /tx/:tx_hash

This endpoint will look for a specific transaction identified by tx_hash.
Example:

```
$ curl -H 'Content-Type: application/json' localhost:30303/tx/c602b2f3b88811bfd7f3fdf866af3b1487bfd21c5b5ea7f7f9a16fb6bb915c24
```

### /tx/shielded

This endpoint returns a list of the shielded assets and their total compiled using all the shielded transactions (in, internal and out)

```
$ curl -H 'Content-Type: application/json' localhost:30303/tx/shielded
```

### /tx/vote_proposal/:proposal_id

This endpoint will look for a vote proposal identified by proposal_id(integer)

```
$ curl -H 'Content-Type: application/json' localhost:30303/tx/vote_proposal/1
```

### /account/updates/:account_id

Retrieve the updates of a specific account.

```
$ curl -H 'Content-Type: application/json' localhost:30303/account/updates/tnam1qqx0p2jus8hjvnxf7clqnn58gcredde02szr9d2r
```


### /address/:address

Retrieve the all the transfers linked to this address (either as a target or source).

```
$ curl -H 'Content-Type: application/json' localhost:30303/address/tnam1qqx0p2jus8hjvnxf7clqnn58gcredde02szr9d2r
```

### /validator/:validator_address/uptime

Return the validator update on certain number of blocks (by default 500 blocks)

```
$ curl -H 'Content-Type: application/json' localhost:30303/validator/tnam1qqx0p2jus8hjvnxf7clqnn58gcredde02szr9d2r/uptime?start=0&end=500
```