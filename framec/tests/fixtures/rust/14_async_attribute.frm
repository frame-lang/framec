pub struct Cache;
impl Cache { pub fn get(&self, k: &str) -> String { format!("v:{}", k) } }

@@[async]
@@system AsyncFetcher {
    interface:
        async fetch(key: String): String

    machine:
        $Ready {
            fetch(key: String): String {
                @@:(@@:self.cache.get(&key))
            }
        }

    domain:
        cache: Cache = Cache
}
