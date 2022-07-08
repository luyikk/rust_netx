```shell
openssl genrsa -aes256 -out CA.key 2048 
openssl req -new -x509 -days 365 -key CA.key -sha256 -out CA.crt
chmod -v 0400 CA.key 
chmod -v 0444 CA.crt
```

### gen server
```shell
   ./tls-gen -s localhost
```

### gen client
```shell
   ./tls-gen -c localhost
```