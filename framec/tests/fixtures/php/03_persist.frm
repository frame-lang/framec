@@[persist(string)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        increment(by: int)
        value(): int = 0

    machine:
        $Counting {
            increment(by: int) {
                @@:self.count = @@:self.count + by
            }
            value(): int {
                @@:(@@:self.count)
            }
        }

    domain:
        count: int = 0
}
