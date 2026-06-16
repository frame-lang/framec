@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        increment(by: Integer)
        value(): Integer = 0

    machine:
        $Counting {
            increment(by: Integer) {
                @@:self.count = @@:self.count + by
            }
            value(): Integer {
                @@:(@@:self.count)
            }
        }

    domain:
        count: Integer = 0
}
