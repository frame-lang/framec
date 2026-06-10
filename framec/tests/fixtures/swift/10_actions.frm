@@system Actions {
    interface:
        increment(n: Int)
        get_total(): Int

    machine:
        $Counting {
            increment(n: Int) {
                self._scale(n)
            }
            get_total(): Int { @@:(@@:self.total) }
        }

    actions:
        _scale(n: Int) {
            @@:self.total = @@:self.total + n * 2
        }

    domain:
        total: Int = 0
}
