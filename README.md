# dshackle_health_adapter

`dshackle` has a few issues with routing batch calls correctly but it's good at monitoring the health of configured nodes in relation to each other.  
Until those issues have been sorted out we will only use `dshackle` for it's health monitoring capabilities with the help of this small tool. It can be configured to report the health of one of the nodes currently enabled in the `dshackle` load balancer.  
Whenever this tool receives a `GET /health` request it will ask `dshackle` for the health of all the nodes, parses the information belonging to the one configured node and returns that health response.  


### Using the tool

The main tool can be built locally with `cargo run -p dshackle_health_adapter` or a container can be built with `docker build --tag dshackle_health_adapter -f ./docker/Dockerfile.binary .`.
Afterwards you just have to pass the correct CLI arguments to the binary:
```
Usage: dshackle_health_adapter [OPTIONS]

Options:
      --bind-address <BIND_ADDRESS>    On which address the server should listen [env: BIND_ADDRESS=] [default: 0.0.0.0:8080]
      --health-url <HEALTH_URL>        Where to read the dshackle detailed health info from [env: HEALTH_URL=] [default: http://127.0.0.1:8082/health?detailed]
      --node-id <NODE_ID>              Name of the node this adapter monitors. This has to match with what is configured in `dshackle.yaml` as the node `id` [env: NODE_ID=] [default: cow-nethermind]
      --unhealthy-lag <UNHEALTHY_LAG>  How many blocks a node may lag behind before being considered unhealthy. If this value is unset we simply use whatever dshackle reports [env: UNHEALTHY_LAG=]
  -h, --help                           Print help information
```

By default it will run on port `8080` and will connect to a `dshackle` instance running on `localhost`. It forwards `/health` requests to `dshackle`'s health endpoint running on port `8082` (default).  
It will report the health of the `cow-nethermind` node but you can easily configure it to report the health of another node that is configured in the `dshackle.yaml` file.

### Implementation details

`dshackle` offers a few ways to get the health of the system
1. `/health` is concerns itself with the overall system and doesn't give any details about individual nodes
2. `/metrics` contains the current lag (number of blocks behind latest known block) for each node
3. `/health?detailed` similar to `/metrics` but with less additional useless information but more reliable in certain error scenarios ([details](https://github.com/cowprotocol/dshackle_health_adapter/pull/2))

This tool is currently using `3` for it's superior reliability but note that the detailed health endpoint is undocumented so the result format might change or the endpoint could get removed altogether.


### Testing

In order to test a `lazy` node that sometimes falls behind on block updated and suddenly catches up again the repo also contains the `lazy_node` binary.  
It can be run with `cargo run -p lazy_node` a few more details can be found [here](https://github.com/cowprotocol/dshackle_health_adapter/pull/1).

```
Usage: lazy_node [OPTIONS]

Options:
      --log-filter <LOG_FILTER>        Log filter to use [env: LOG_FILTER=] [default: warn,lazy_node=debug]
      --bind-address <BIND_ADDRESS>    On which address the server should listen [env: BIND_ADDRESS=] [default: 0.0.0.0:9545]
      --node-url <NODE_URL>            Upstream JSON RPC node to proxy [env: NODE_URL=] [default: http://127.0.0.1:8545]
      --update-chance <UPDATE_CHANCE>  Chance to update a new block [env: UPDATE_CHANCE=] [default: 0.25]
  -h, --help                           Print help information
```
