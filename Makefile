all: build

build:
	wasm-pack build --release --target web --out-name wasm --out-dir ./static

serve:
	cd static && \
		python3 -m http.server 8000

tmp/lotw-user-activity.csv:
	mkdir -p tmp/
	curl https://lotw.arrl.org/lotw-user-activity.csv -o tmp/lotw-user-activity.csv

tmp/l_amat.zip:
	mkdir -p tmp/
	curl ftp://wirelessftp.fcc.gov/pub/uls/complete/l_amat.zip -o tmp/l_amat.zip

l_amat: tmp/l_amat.zip
	cd tmp \
		&& unzip -o l_amat.zip

db: tmp/lotw-user-activity.csv l_amat
	cd tmp \
		&& python3 ../scripts/import-en.py \
		&& python3 ../scripts/import-lotw.py \
		&& python3 ../scripts/import-am.py \
		&& python3 ../scripts/lotw-file.py
	rm -rf static/out/
	mv tmp/out static/
	mv tmp/lotw-users.dat static/out/
	cp scripts/states.json static/out/states.json

clean:
	rm -rf tmp/
