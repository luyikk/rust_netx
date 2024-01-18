```shell
openssl genrsa -aes256 -out CA.key 2048 
openssl req -new -x509 -days 3650 -key CA.key -sha256 -out CA.crt
chmod -v 0400 CA.key 
chmod -v 0444 CA.crt
```
or
```shell
openssl req -x509 -newkey rsa:4096 -days 365 -keyout CA.key -out CA.crt
chmod -v 0400 CA.key 
chmod -v 0444 CA.crt
```


### gen server
```shell
   ./tls_gen.sh -s localhost
```

### gen client
```shell
   ./tls_gen.sh -c localhost
```


edit server-key.pem
```
-----BEGIN PRIVATE KEY-----
```
to
```
-----BEGIN RSA PRIVATE KEY-----
```

```
-----END PRIVATE KEY-----
```
to
```
-----END RSA PRIVATE KEY-----
```


可使用
```
openssl pkcs12 -export -out certificate.pfx -inkey key.pem -in cert.pem
``` 
转换成 pfx文件