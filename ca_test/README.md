```shell
openssl genrsa -aes256 -out CA.key 2048 
openssl req -new -x509 -days 3650 -key CA.key -sha256 -out CA.crt
chmod -v 0400 CA.key 
chmod -v 0444 CA.crt
```
or
```shell
openssl req -x509 -newkey rsa:4096 -days 3650 -keyout CA.key -out CA.crt
chmod -v 0400 CA.key 
chmod -v 0444 CA.crt
```

## 如果要生成v3 版本的证书请使用 ./v3/tls_gen.sh 否则不会被rustls 0.23.0 之后的版本接受

```shell
chmod 777 tls_gen.sh 
```

### gen server
```shell
   ./tls_gen.sh -s localhost
```

### gen client
```shell
   ./tls_gen.sh -c localhost
```



可使用
```
openssl pkcs12 -export -out certificate.pfx -inkey key.pem -in cert.pem
``` 
转换成 pfx文件  

下面生成jks
```shell
openssl pkcs12 -inkey client-key.pem -in  client-crt.pem -export -out cert.p12 -legacy
java -cp jetty-6.1.26.jar org.mortbay.jetty.security.PKCS12Import cert.p12 keystore.jks
openssl x509 -outform der -in client-crt.pem -out cert.der
keytool -import -alias test -keystore truststore.jks -file cert.der
```

truststore.jks  
keystore.jks is jdk keystore file

