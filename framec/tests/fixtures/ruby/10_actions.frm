@@system Actions {
    interface:
        increment(n: Integer)
        get_total(): Integer

    machine:
        $Counting {
            increment(n: Integer) {
                self._scale(n)
            }
            get_total(): Integer { @@:(@@:self.total) }
        }

    actions:
        _scale(n: Integer) {
            @@:self.total = @@:self.total + n * 2
        }

    domain:
        total: Integer = 0
}
