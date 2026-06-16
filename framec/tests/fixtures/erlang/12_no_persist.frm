@@[persist(string)]
@@[save(snapshot)]
@@[load(restore)]
@@system NoPersist {
    interface:
        bump()
        set_cache(v: integer)
        get_count(): integer
        get_cache(): integer

    machine:
        $Active {
            bump() { @@:self.count = @@:self.count + 1 }
            set_cache(v: integer) { @@:self.cache = v }
            get_count(): integer { @@:(@@:self.count) }
            get_cache(): integer { @@:(@@:self.cache) }
        }

    domain:
        count: integer = 0
        @@[no_persist]
        cache: integer = -1
}
