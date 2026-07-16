default:
	echo "Hello, world!"

# generate and serve static website
serve:
	zola --root www serve --base-url localhost

[default]
hi:
	echo "hi"
