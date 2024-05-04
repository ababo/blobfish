#!/bin/bash

# Tested with Debian 12 servers.

set -ex

if [ -z "$SSH_ADDRESS" ]; then
    echo "no SSH_ADDRESS env var is set"
    exit 1
fi

if [ -z "$SERVER_ADDRESS" ]; then
    echo "no SERVER_ADDRESS env var is set, using $SSH_ADDRESS"
    SERVER_ADDRESS="$SSH_ADDRESS"
fi

ssh root@$SSH_ADDRESS 'apt-get update'
ssh root@$SSH_ADDRESS '
apt-get install -y \
    libgomp1 \
    libsndfile1-dev \
    pip
'
ssh root@$SSH_ADDRESS '
pip install --break-system-packages \
    onnxruntime \
    pyannote-audio \
    tornado
'

scp -r infsrv "root@[$SSH_ADDRESS]:/root/"
scp -r model "root@[$SSH_ADDRESS]:/root/"

ssh root@$SSH_ADDRESS "
cat > /etc/systemd/system/blobfish-infsrv.service <<EOF
[Unit]
Description=Blobfish Inference Server
After=network.target

[Service]
Type=simple
Environment="LOG_LEVEL=debug"
Environment="SERVER_ADDRESS=$SERVER_ADDRESS"
ExecStart=python3 infsrv
WorkingDirectory=/root

[Install]
WantedBy=multi-user.target
EOF
"

ssh root@$SSH_ADDRESS 'systemctl daemon-reload'
ssh root@$SSH_ADDRESS 'systemctl start blobfish-infsrv'
ssh root@$SSH_ADDRESS 'systemctl enable blobfish-infsrv'
