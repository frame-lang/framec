@@[persist(string)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        increment(by: integer)
        value(): integer = 0

    machine:
        $Counting {
            increment(by: integer) {
                @@:self.count = @@:self.count + by
            }
            value(): integer {
                @@:(@@:self.count)
            }
        }

    domain:
        count: integer = 0
}
