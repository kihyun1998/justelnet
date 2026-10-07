# syntax=docker/dockerfile:1
# GNU inetutils telnetd 2.8, built from the signed release, run by inetutils inetd.

FROM debian@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends gcc make libc6-dev libncurses-dev \
 && rm -rf /var/lib/apt/lists/*
ADD --checksum=sha256:57b3cf4f77555992881e5ba2a09a63b05aa2c56342a60ed4305b5f45938390b5 \
    https://ftp.gnu.org/gnu/inetutils/inetutils-2.8.tar.gz /inetutils-2.8.tar.gz
RUN tar -xzf /inetutils-2.8.tar.gz \
 && cd inetutils-2.8 \
 && ./configure --disable-servers --disable-clients --enable-telnetd --enable-inetd \
 && make -j"$(nproc)" \
 && make install DESTDIR=/out

FROM debian@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
RUN apt-get update \
 && apt-get install -y --no-install-recommends login netbase \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/usr/local/ /usr/local/
# The test account.
RUN useradd --create-home --shell /bin/bash e2e \
 && echo 'e2e:justelnet-e2e' | chpasswd \
 && echo "PS1='e2e\$ '" >> /home/e2e/.bashrc \
 && echo 'telnet stream tcp nowait root /usr/local/libexec/telnetd telnetd' > /etc/inetd.conf
EXPOSE 23
CMD ["/usr/local/libexec/inetd", "--foreground", "/etc/inetd.conf"]
