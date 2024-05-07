#!/bin/bash

# Tested with Debian 12 servers.

set -ex

if [ -z "$CAPABILITIES" ]; then
    echo "no CAPABILITIES env var is set"
    exit 1
fi

if [ -z "$LOG_LEVEL" ]; then
    echo "no LOG_LEVEL env var is set, using info"
    LOG_LEVEL=info
fi

if [ -z "$SSH_ADDRESS" ]; then
    echo "no SSH_ADDRESS env var is set"
    exit 1
fi

if [ -z "$SERVER_ADDRESS" ]; then
    echo "no SERVER_ADDRESS env var is set, using $SSH_ADDRESS"
    SERVER_ADDRESS="$SSH_ADDRESS"
fi

if [ -z "$SERVER_PORT" ]; then
    echo "no SERVER_PORT env var is set, using 80"
    SERVER_PORT=80
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
    dataclasses_json \
    onnxruntime \
    pyannote-audio \
    tornado
'

ssh root@$SSH_ADDRESS 'mkdir -p blobfish'
scp -r infsrv "root@[$SSH_ADDRESS]:/root/blobfish/"

ssh root@$SSH_ADDRESS 'mkdir -p blobfish/model'
model_dirs=$(cat infsrv/capability.json |
  jq --arg caps_list $CAPABILITIES '
    ($caps_list | split(",")) as $caps |
    .capabilities |
    to_entries[] |
    select(.key as $k | $caps | index($k)) |
    .value.modelDirs' |
  jq -sr 'flatten | unique | .[]')
while IFS= read -r model_dir; do
    scp -r "$model_dir" "root@[$SSH_ADDRESS]:/root/blobfish/model/"
done <<< "$model_dirs"

ssh root@$SSH_ADDRESS "
cat > /etc/systemd/system/blobfish-infsrv.service <<EOF
[Unit]
Description=Blobfish Inference Server
After=network.target

[Service]
Type=simple
Environment="CAPABILITIES=$CAPABILITIES"
Environment="LOG_LEVEL=$LOG_LEVEL"
Environment="SERVER_ADDRESS=$SERVER_ADDRESS"
Environment="SERVER_PORT=$SERVER_PORT"
ExecStart=python3 infsrv
WorkingDirectory=/root/blobfish

[Install]
WantedBy=multi-user.target
EOF
"

ssh root@$SSH_ADDRESS 'systemctl daemon-reload'
ssh root@$SSH_ADDRESS 'systemctl start blobfish-infsrv'
ssh root@$SSH_ADDRESS 'systemctl enable blobfish-infsrv'
