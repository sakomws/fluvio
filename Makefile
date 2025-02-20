TOOLCHAIN = "./rust-toolchain"
RUSTV = $(shell cat ${TOOLCHAIN})
RUST_DOCKER_IMAGE=fluvio/rust-tool:${RUSTV}
CARGO_BUILD=build
BIN_NAME=debug
PUSH=push
GITHUB_USER=infinyon
GITHUB_REPO=fluvio
GITHUB_TAG=0.1.0-alpha
TARGET_LINUX=x86_64-unknown-linux-musl
TARGET_DARWIN=x86_64-apple-darwin
CLI_BUILD=fluvio_cli

run-all-unit-test:
	cargo test --all

install_musl:
	rustup target add ${TARGET_LINUX}

clean_build:
	rm -rf /tmp/cli-*

# create binaries for CLI
release_cli_darwin:	
	cargo build --release --bin fluvio  --target ${TARGET_DARWIN}
	mkdir -p /tmp/$(CLI_BUILD)_${TARGET_DARWIN}
	cp target/${TARGET_DARWIN}/release/fluvio /tmp/$(CLI_BUILD)_${TARGET_DARWIN}
	cd /tmp;tar -czvf cli-${TARGET_DARWIN}-release.tar.gz $(CLI_BUILD)_${TARGET_DARWIN};rm -rf $(CLI_BUILD)_${TARGET_DARWIN}

release_cli_linux:
	cargo build --release --bin fluvio  --target ${TARGET_LINUX}
	mkdir -p /tmp/$(CLI_BUILD)_${TARGET_LINUX}
	cp target/${TARGET_LINUX}/release/fluvio /tmp/$(CLI_BUILD)_${TARGET_LINUX}
	cd /tmp;tar -czvf cli-${TARGET_LINUX}-release.tar.gz $(CLI_BUILD)_${TARGET_LINUX};rm -rf $(CLI_BUILD)_${TARGET_LINUX}



# create docker images for release
release_image:	CARGO_BUILD=build --release
release_image:	PUSH=push_release
release_image:	BIN_NAME=release

debug_image:	linux-spu-server spu_image linux-sc-server sc_image
release_image:	linux-spu-server spu_image spu_image linux-sc-server sc_image



linux-sc-server:
	cargo $(CARGO_BUILD) --bin sc-server  --target ${TARGET_LINUX}

linux-spu-server:
	cargo $(CARGO_BUILD) --bin spu-server  --target ${TARGET_LINUX}


spu_image:	install_musl linux-spu-server
	make build BIN_NAME=$(BIN_NAME) $(PUSH) -C k8-util/docker/spu

sc_image:	install_musl linux-spu-server
	make build BIN_NAME=$(BIN_NAME) $(PUSH) -C k8-util/docker/sc
	

cargo_cache_dir:
	mkdir -p .docker-cargo


# run test in docker
docker_linux_test:	cargo_cache_dir
	 docker run --rm --volume ${PWD}:/src --workdir /src  \
	 	-e USER -e CARGO_HOME=/src/.docker-cargo \
		-e CARGO_TARGET_DIR=/src/target-docker \
	  	${RUST_DOCKER_IMAGE} cargo test


# create releases
# release CLI can be downloaded from https://github.com/aktau/github-release/releases
create_release:
	github-release release \
		--user ${GITHUB_USER} \
		--repo ${GITHUB_REPO} \
		--tag ${GITHUB_TAG} \
		--name "${GITHUB_TAG}" \
		--description "${GITHUB_TAG}"


upload_release:	release_cli_darwin release_cli_linux
	github-release upload \
		--user ${GITHUB_USER} \
		--repo ${GITHUB_REPO} \
		--tag ${GITHUB_TAG} \
		--name "cli-${TARGET_DARWIN}-release.tar.gz" \
		--file /tmp/cli-${TARGET_DARWIN}-release.tar.gz 
	github-release upload \
		--user ${GITHUB_USER} \
		--repo ${GITHUB_REPO} \
		--tag ${GITHUB_TAG} \
		--name "cli-${TARGET_LINUX}-release.tar.gz" \
		--file /tmp/cli-${TARGET_LINUX}-release.tar.gz


delete_release:
	github-release delete \
	--user ${GITHUB_USER} \
	--repo ${GITHUB_REPO} \
	--tag ${GITHUB_TAG}


## Helper targets to compile specific crate


build-sc-test:
	cd sc-server;cargo test --no-run
			
			
build-spu-test:
	cd spu-server;cargo test --no-run

build-storage-test:
	cd storage;cargo test --no-run

build-internal-test:
	cd internal-api;cargo test --no-run	

		
test-spu:
	cd spu-server;cargo test

test-spu-offset:
	cd spu-server;RUST_LOG=spu_server=trace cargo test flv_offset_fetch_test	

test-sc-connection:
	cd sc-server;RUST_LOG=sc_server=trace cargo test connection_test

test-sc-partition:
	cd sc-server;RUST_LOG=sc_server=trace cargo test partition_test

test-sc-controller:
	cd sc-server; cargo test test_controller_basic		

test-sc:
	cd sc-server;cargo test		

test-storage:
	cd storage;cargo test

test-internal-api:
	cd api/internal-api;cargo test

test-cli:
	cd cli;cargo test

test-helper:
	cd future-helper;cargo test

test-aio:
	cd future-aio;cargo test

test-kfsocket:
	cd kf-socket;cargo test

test-kfservice:
	cd kf-service;cargo test

test-k8client:
	cd k8-client;cargo test

test-k8config:
	cd k8-config;cargo test

.PHONY:	test-helper teste-aio test-kfsocket test-kfservice test-k8client test-k8config