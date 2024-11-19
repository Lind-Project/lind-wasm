.PHONY: test clean

test: clean
	chmod +x gen_netdevs.sh
	./gen_netdevs.sh
	cargo test --lib

clean:
	rm -f gen_netdevs net_devices
	rm -f linddata.*
	rm -f lind.metadata

format:
	cargo fmt -- --check
