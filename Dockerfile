FROM ubuntu:latest
MAINTAINER melvinto@gmail.com

RUN apt update
RUN apt install -y ca-certificates
RUN apt clean

COPY target/release/rpromhub /usr/bin/rpromhub
RUN mkdir /etc/rpromhub
COPY Settings.toml /etc/rpromhub/Settings.toml

CMD /usr/bin/rpromhub -c /etc/rpromhub/Settings.toml
