#!/bin/bash

# Generates and signs a Docker client or server key 
# Clients: eg. Stevedore, Antykethera, Laptops
# Servers: System's Docker hosts, AWS hosts

### SOME FUNCTIONS ###

f_warn() {
  echo $1
}

f_err() {
  echo $1
  exit 1
}

f_test_perms() {
  PERMS=$(stat "$1" | sed -n '/^Access: (/{s/Access: (\([0-9]\+\).*$/\1/;p}')
  #if [[ $PERMS != "$2" ]] ; then
  #  f_err "File permission on ${1} are too open; should be ${2}"
  #fi
}

f_test_file() {
  if [[ ! -f "$1" ]] ; then
    f_err "${1} doesn't exist in the current directory"
  elif [[ ! -r "$1" ]] ; then
    f_err "${1} is not readable (are you root?)" 
  fi
}

### VARIABLES ###

SCRIPT="$(basename $0)"
VERSION='0.2'

USAGE="
  Syntax: $SCRIPT [options] [hostname]

  Options:

  -h		show this message
  -v		print version information
  -s		generate server cert (excludes client)
  -c		generate client cert (excludes server)
"
while getopts 'hvs:c:' OPTION; do
  case "$OPTION" in
    h) echo $USAGE
       exit 0
       ;;
    v) echo $SCRIPT v${VERSION}
       exit 0
       ;;
    s) HOST="$OPTARG"
       FUNCTION='server'
       SUBJSTR="/CN=${HOST}"
       ;;
    c) HOST="$OPTARG"
       FUNCTION='client'
       SUBJSTR='/CN=client'
       ;;
    esac
done

HOST_STRING="${FUNCTION}_${HOST}"

CAKEY="CA.key"
CACRT="CA.crt"

KEY="${HOST_STRING}-key.pem"
CSR="${HOST_STRING}.csr"
CRT="${HOST_STRING}-crt.pem"

### OPENSSL OPTIONS ###
KEYLEN="2048"
DAYS="3650"
EXTCONF='/tmp/extfile.cnf'

# Script should be run as root
echo "TODO: Add test for sudo/root"

# Script needs an argument

if [[ -z $HOST ]] ; then
  f_err "$USAGE"
fi

if [[ -z $FUNCTION ]] ; then
  f_warn "No function specified; generating client creds"
  FUNCTION='client'
  SUBJSTR='/CN=client'
fi

# Make sure the CA KEY and CRT are present, readable, and 
# that they have secure permissions

f_test_file $CAKEY
f_test_perms $CAKEY 0400

f_test_file $CACRT
f_test_perms $CACRT 0444

# Add the extension conf file if this is a client cert
if [[ "s_$FUNCTION" == "s_client" ]] ; then
  # Create extensions conf file
  if [[ -f $EXTCONF ]] ; then
    f_err "${EXTCONF} already exists"
  else
    echo 'extendedKeyUsage = clientAuth' > $EXTCONF \
    || f_err "Unable to create ${EXTCONF}"
  fi
  # Generate the KEY
  openssl genrsa -out $KEY $KEYLEN
  # Generate a CSR
  openssl req -subj "${SUBJSTR}" -new -key $KEY -out $CSR
  # Sign the CRT with our CA and an extensions conf
  openssl x509 -req -days $DAYS -in $CSR -CA $CACRT -CAkey $CAKEY \
  -CAcreateserial -out $CRT -extfile $EXTCONF
  rm -v $EXTCONF
elif [[ "s_${FUNCTION}" == "s_server" ]] ; then
  # Generate the KEY
  openssl genrsa -out $KEY $KEYLEN
  # Generate a CSR
  openssl req -subj "${SUBJSTR}" -new -key $KEY -out $CSR
  # Sign the CRT with our CA
  openssl x509 -req -days $DAYS -in $CSR -CA $CACRT -CAkey $CAKEY \
  -CAcreateserial -out $CRT 
else
  f_err "Unknown Function" 
fi

chmod 0400 $KEY
chmod 0444 $CRT
rm -v $CSR
