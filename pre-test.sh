#!/bin/bash

mkdir -p /home/lind-wasm/src/RawPOSIX/tmp/testfiles/
touch /home/lind-wasm/src/RawPOSIX/tmp/testfiles/readlinkfile.txt
ln -s src/RawPOSIX/tmp/testfiles/readlinkfile.txt src/RawPOSIX/tmp/testfiles/readlinkfile
