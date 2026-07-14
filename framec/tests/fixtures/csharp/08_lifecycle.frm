@@system Lifecycle {
    interface:
        start(label: string)
        stop()

    machine:
        $Idle {
            start(label: string) {
                -> (label) $Running
            }
        }

        $Running {
            $>(label: string) {
                @@:self.entered = @@:self.entered + 1;
                @@:self.tag = label;
            }
            <$() {
                @@:self.exited = @@:self.exited + 1
            }
            stop() {
                -> $Idle
            }
        }

    domain:
        entered: int = 0
        exited: int = 0
        tag: string = ""
}
