# SparkSDR Websocket Demo

A simple demo using SparkSDR's websocket interface.

![Main Window](https://raw.githubusercontent.com/nricciar/sparksdr-websocket-demo/master/static/screenshot.png)

## Usage

```
make build # builds wasm/js files and places them in `static/` dir
make db # optional - us callsign json files (need 6GB of free storage)
make serve # runs a small web server on port 8000 serving the files in `static/`
```

Make sure web sockets are enabled in SparkSDR and then load http://localhost:8000 in your browser.

### `make db`

Running `make db` will download current FCC and LoTW records to create a collection of json files for each US callsign.  This will consume a large amount of storage space (just under 6GB). Will place the generated json files in `static/out`.

`make clean` will remove any fcc/lotw downloads and any temporary files generated during the process.  This will not remove the json files.