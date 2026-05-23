@@[persist(String)]
@@[save(snapshot)]
@@[load(restore)]
@@system Counter {
    interface:
        increment(by: Int)
        value(): Int = 0

    machine:
        $Counting {
            increment(by: Int) {
                self.count = self.count + by
            }
            value(): Int {
                @@:(self.count)
            }
        }

    domain:
        count: Int = 0
}
