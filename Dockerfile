# pi-kit v0.4.1
# The binary is cross-compiled on the laptop (`just build`), so the image is only that file:
# no shell, no libc, no package manager. Built for linux/arm64 (`just image`).
FROM scratch
COPY --chmod=0555 dist/app /app
USER 65534:65534
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD ["/app", "healthcheck"]
ENTRYPOINT ["/app"]
CMD ["serve"]
