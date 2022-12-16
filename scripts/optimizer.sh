:

U="cosmwasm"
V="0.12.10"

M=$(uname -m)
M="x86_64"
A="linux/${M/x86_64/amd64}"
S=${M#x86_64}
S=${S:+-$S}

docker run --platform $A --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  $U/workspace-optimizer$S:$V
