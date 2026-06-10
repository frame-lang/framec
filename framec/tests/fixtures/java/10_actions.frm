@@system Actions {
    interface:
        increment(n: int)
        get_total(): int

    machine:
        $Counting {
            increment(n: int) {
                self._scale(n)
            }
            get_total(): int { @@:(@@:self.total) }
        }

    actions:
        _scale(n: int) {
            self.total = self.total + n * 2
        }

    domain:
        total: int = 0
}
