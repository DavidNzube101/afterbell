FROM ubuntu:24.04 AS build

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake g++ make pkg-config libcurl4-openssl-dev libdrogon-dev libjsoncpp-dev libpq-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY backend ./backend
RUN cmake -S backend -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=OFF -DAFTERBELL_REQUIRE_DROGON=ON \
    && cmake --build build --target afterbell-api afterbell-indexer --parallel 2

FROM ubuntu:24.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libcurl4 libdrogon1t64 libjsoncpp25 libpq5 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin afterbell

WORKDIR /app
COPY --from=build /app/build/afterbell-api ./afterbell-api
COPY --from=build /app/build/afterbell-indexer ./afterbell-indexer
COPY --from=build /app/backend/rh_stocks_addresses.json ./backend/rh_stocks_addresses.json
USER afterbell
ENV PORT=8080
EXPOSE 8080
CMD ["./afterbell-api"]
