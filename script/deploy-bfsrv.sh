#!/bin/bash

# Tested with Ubuntu 24.04 LTS servers.

set -ex

if [ -z "$ADMIN_EMAIL" ]; then
    ADMIN_EMAIL=admin@blobfish.no
    echo "no ADMIN_EMAIL env var is set, using $ADMIN_EMAIL"
fi

if [ -z "$API_DOMAIN" ]; then
    API_DOMAIN=api.blobfish.no
    echo "no API_DOMAIN env var is set, using $API_DOMAIN"
fi

if [ -z "$CURRENCY" ]; then
    echo "no CURRENCY env var is set, using USD"
    CURRENCY=USD
fi

if [ -z "$DATABASE_NAME" ]; then
    DATABASE_NAME=blobfish
    echo "no DATABASE_NAME env var is set, using $DATABASE_NAME"
fi

if [ -z "$DATABASE_URL" ]; then
    DATABASE_URL="postgres://root:root@127.0.0.1/$DATABASE_NAME"
    echo "no DATABASE_URL env var is set, using $DATABASE_URL"
fi

if [ -z "$PAYPAL_CANCEL_URL" ]; then
    PAYPAL_CANCEL_URL=https://blobfish.no/finish_payment.html
    echo "no PAYPAL_CANCEL_URL env var is set, using $PAYPAL_CANCEL_URL"
fi

if [ -z "$PAYPAL_CLIENT_ID" ]; then
    echo "no PAYPAL_CLIENT_ID env var is set"
    exit 1
fi

if [ -z "$PAYPAL_RETURN_URL" ]; then
    PAYPAL_RETURN_URL=https://blobfish.no/finish_payment.html
    echo "no PAYPAL_RETURN_URL env var is set, using $PAYPAL_RETURN_URL"
fi

if [ -z "$PAYPAL_SANDBOX" ]; then
    PAYPAL_SANDBOX=true
    echo "no PAYPAL_RETURN_URL env var is set, using $PAYPAL_SANDBOX"
fi

if [ -z "$PAYPAL_SECRET_KEY" ]; then
    echo "no PAYPAL_SECRET_KEY env var is set"
    exit 1
fi

if [ -z "$RUST_LOG" ]; then
    RUST_LOG=debug
    echo "no RUST_LOG env var is set, using $RUST_LOG"
fi

if [ -z "$SERVER_ADDRESS" ]; then
    SERVER_ADDRESS="127.0.0.1:9321"
    echo "no SERVER_ADDRESS env var is set, using $SERVER_ADDRESS"
fi

if [ -z "$SMTP_FROM" ]; then
    SMTP_FROM=noreply@blobfish.no
    echo "no SMTP_FROM env var is set, using $SMTP_FROM"
fi

if [ -z "$SMTP_USERNAME" ]; then
    echo "no SMTP_USERNAME env var is set"
    exit 1
fi

if [ -z "$SMTP_PASSWORD" ]; then
    echo "no SMTP_PASSWORD env var is set"
    exit 1
fi

if [ -z "$SMTP_RELAY" ]; then
    SMTP_RELAY=mail.blobfish.no
    echo "no SMTP_RELAY env var is set, using $SMTP_RELAY"
fi

if [ -z "$SSH_ADDRESS" ]; then
    echo "no SSH_ADDRESS env var is set, using $API_DOMAIN"
    SSH_ADDRESS=$API_DOMAIN
fi

ssh root@$SSH_ADDRESS 'apt-get update'
ssh root@$SSH_ADDRESS '
apt-get install -y \
    build-essential \
    certbot \
    libssl-dev \
    nginx \
    pkg-config \
    postgresql
'

ssh root@$SSH_ADDRESS "certbot certonly --webroot -w /var/www/html \
    -d $API_DOMAIN -m $ADMIN_EMAIL --agree-tos --non-interactive"

ssh root@$SSH_ADDRESS "
cat > /etc/nginx/nginx.conf <<EOF
worker_processes auto;
user www-data;

events {
    use epoll;
    worker_connections 1024;
}

error_log /var/log/nginx/error.log info;

http {
    server_tokens off;
    include mime.types;
    charset utf-8;

    access_log /var/log/nginx/access.log combined;

    server {
        listen 443 ssl;
        server_name $API_DOMAIN;

        ssl_certificate /etc/letsencrypt/live/$API_DOMAIN/fullchain.pem;
        ssl_certificate_key /etc/letsencrypt/live/$API_DOMAIN/privkey.pem;

        root /var/www/html;

        location / {
            try_files \\\$uri \\\$uri/ @backend;
        }

        location @backend {
            proxy_pass http://$SERVER_ADDRESS;
            proxy_http_version 1.1;
            proxy_set_header Upgrade \\\$http_upgrade;
            proxy_set_header Connection \"upgrade\";
        }
    }
}
EOF
"

ssh root@$SSH_ADDRESS 'nginx -t && systemctl reload nginx'

ssh root@$SSH_ADDRESS 'rm -rf blobfish && mkdir blobfish'
scp -r bfsrv Cargo.* "root@[$SSH_ADDRESS]:/root/blobfish/"

ssh root@$SSH_ADDRESS DATABASE_NAME=$DATABASE_NAME '
    sudo -u postgres psql -c "CREATE ROLE root \
        WITH CREATEDB LOGIN SUPERUSER PASSWORD '"'"'root'"'"'"
    if [ $? -eq 0 ]; then
        export PGPASSWORD=root
        psql -U root -d postgres -c "CREATE DATABASE $DATABASE_NAME"
        psql -U root -d $DATABASE_NAME -f blobfish/bfsrv/schema.sql
    fi
'

ssh root@$SSH_ADDRESS "curl --proto '=https' \
    --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"

ssh root@$SSH_ADDRESS '
    cd blobfish && \
    $HOME/.cargo/bin/cargo build --release && \
    rm -r bfsrv Cargo.* && \
    mv target/release/bfsrv . && \
    rm -r target
'

ssh root@$SSH_ADDRESS "
cat > /etc/systemd/system/blobfish-bfsrv.service <<EOF
[Unit]
Description=Blobfish API Server
After=network.target
After=postgresql.service

[Service]
Type=simple
Environment="CURRENCY=$CURRENCY"
Environment="DATABASE_URL=$DATABASE_URL"
Environment="SERVER_ADDRESS=$SERVER_ADDRESS"
Environment="PAYPAL_CANCEL_URL=$PAYPAL_CANCEL_URL"
Environment="PAYPAL_CLIENT_ID=$PAYPAL_CLIENT_ID"
Environment="PAYPAL_RETURN_URL=$PAYPAL_RETURN_URL"
Environment="PAYPAL_SANDBOX=$PAYPAL_SANDBOX"
Environment="PAYPAL_SECRET_KEY=$PAYPAL_SECRET_KEY"
Environment="RUST_LOG=$RUST_LOG"
Environment="SMTP_FROM=$SMTP_FROM"
Environment="SMTP_USERNAME=$SMTP_USERNAME"
Environment="SMTP_PASSWORD=$SMTP_PASSWORD"
Environment="SMTP_RELAY=$SMTP_RELAY"

ExecStart=/root/blobfish/bfsrv
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF
"

ssh root@$SSH_ADDRESS 'systemctl daemon-reload'
ssh root@$SSH_ADDRESS 'systemctl restart blobfish-bfsrv || \
    systemctl start blobfish-bfsrv'
ssh root@$SSH_ADDRESS 'systemctl enable blobfish-bfsrv'

sleep 5
ssh root@$SSH_ADDRESS 'systemctl status blobfish-bfsrv'
