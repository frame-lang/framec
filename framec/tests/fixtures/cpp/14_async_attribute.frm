@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: std::string): std::string

    machine:
        $Ready {
            fetch(key: std::string): std::string {
                @@:(self.cache.get(key))
            }
        }

    domain:
        cache: Cache = nil
}
