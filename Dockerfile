FROM gcr.io/distroless/cc-debian12
COPY fv /usr/local/bin/fv
ENTRYPOINT ["fv"]
