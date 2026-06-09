@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: str): str

    machine:
        $Ready {
            fetch(key: str): str {
                @@:(self.cache.get(key))
            }
        }

    domain:
        cache: Cache = nil
}
