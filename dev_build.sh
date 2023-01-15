sudo docker build -f docker/Dockerfile.dev -t sentiment-frontend:dev docker
sudo docker run -it --mount type=bind,source=$(pwd)/code,target=/build sentiment-frontend:dev /bin/bash -c "cargo build --release --target wasm32-unknown-unknown && wasm-bindgen --out-name sentiment-frontend --out-dir pkg --target web target/wasm32-unknown-unknown/release/sentiment_frontend.wasm"
sudo docker build -f docker/Dockerfile.serve -t sentiment-frontend:serve code/
sudo docker run --network=host sentiment-frontend:serve