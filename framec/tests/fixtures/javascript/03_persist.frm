@@[persist(string)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        increment(by: number)
        value(): number = 0

    machine:
        $Counting {
            increment(by: number) {
                @@:self.count = @@:self.count + by
            }
            value(): number {
                @@:(@@:self.count)
            }
        }

    domain:
        count: number = 0
}
