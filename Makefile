all: build

build:
	wasm-pack build --release --target web --out-name wasm --out-dir ./static

serve:
	cd static && \
		python3 -m http.server 8000

