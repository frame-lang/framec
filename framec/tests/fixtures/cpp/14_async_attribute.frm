#include <string>
struct Cache {
    std::string get(const std::string& k) { return "v:" + k; }
};

@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: std::string): std::string

    machine:
        $Ready {
            fetch(key: std::string): std::string {
                @@:(@@:self.cache.get(key))
            }
        }

    domain:
        cache: Cache = nullptr
}
