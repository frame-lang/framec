@@system Lifecycle {
    interface:
        start(label: char*)
        stop()

    machine:
        $Idle {
            start(label: char*) {
                (label)
                -> $Running
            }
        }

        $Running {
            $>(label: char*) {
                self.entered = self.entered + 1;
                self.tag = label;
            }
            <$() {
                self.exited = self.exited + 1
            }
            stop() {
                -> $Idle
            }
        }

    domain:
        entered: int = 0
        exited: int = 0
        tag: char* = ""
}
