
FROM rust:1.82-bullseye
RUN mkdir /code
WORKDIR /code
RUN apt update
RUN apt install cmake clang libpython3.9-dev -y
ADD . /code/