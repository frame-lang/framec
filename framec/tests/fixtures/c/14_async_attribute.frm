typedef struct { int _; } Cache;

@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: char*): char*

    machine:
        $Ready {
            fetch(key: char*): char* {
                @@:(@@:self.cache.get(key))
            }
        }

    domain:
        cache: Cache = NULL
}
