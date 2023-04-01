sudo docker build -f docker/Dockerfile.dev -t horta-frontend:dev docker
sudo docker run -it --mount type=bind,source=$(pwd)/code,target=/build horta-frontend:dev /bin/bash -c "cargo build --release --target wasm32-unknown-unknown && wasm-bindgen --out-name horta-frontend --out-dir pkg --target web target/wasm32-unknown-unknown/release/horta_frontend.wasm"
sudo docker build -f docker/Dockerfile.serve -t horta-frontend:serve code/
sudo docker run --network=host horta-frontend:serve