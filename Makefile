
release:
	cargo build --release
	sudo docker build -t rpromhub .
