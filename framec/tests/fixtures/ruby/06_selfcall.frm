@@system SelfCall {
    interface:
        kick()
        report(): Integer

    machine:
        $Active {
            kick() {
                @@:self.count = @@:self.count + 1
                @@:self.report()
            }
            report(): Integer { @@:(@@:self.count) }
        }

    domain:
        count: Integer = 0
}
