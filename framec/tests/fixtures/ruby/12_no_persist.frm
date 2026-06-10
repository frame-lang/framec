@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system NoPersist {
    interface:
        bump()
        set_cache(v: Integer)
        get_count(): Integer
        get_cache(): Integer

    machine:
        $Active {
            bump() { @@:self.count = @@:self.count + 1 }
            set_cache(v: Integer) { @@:self.cache = v }
            get_count(): Integer { @@:(@@:self.count) }
            get_cache(): Integer { @@:(@@:self.cache) }
        }

    domain:
        count: Integer = 0
        @@[no_persist]
        cache: Integer = -1
}
